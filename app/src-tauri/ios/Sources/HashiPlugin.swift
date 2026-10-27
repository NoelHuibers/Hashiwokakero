import GameKit
import Tauri
import UIKit
import WebKit

struct SystemBarsArgs: Decodable {
  let dark: Bool
}

struct ShareArgs: Decodable {
  let text: String
  /// A file name: shares `text` as this file instead of as text.
  let file: String?
}

/// An achievement as the page reports it (src/api/games.rs): steps of a target, or earned.
struct GamesAchievement: Decodable {
  let id: String
  let steps: UInt32?
  let target: UInt32?
  let earned: Bool

  /// Game Center's percentComplete.
  var percent: Double {
    if earned { return 100 }
    guard let steps, let target, target > 0 else { return 0 }
    return min(100, Double(steps) * 100 / Double(target))
  }
}

struct GamesScore: Decodable {
  let id: String
  let value: Int64
}

struct GamesReportArgs: Decodable {
  let achievements: [GamesAchievement]
  let scores: [GamesScore]
}

struct GamesShowArgs: Decodable {
  /// "achievements" or "leaderboards".
  let kind: String
}

struct GamesStatus: Encodable {
  let available: Bool
  let signedIn: Bool
}

// The app's own plugin ("hashi" in src/lib.rs) on iOS, HashiPlugin.kt's counterpart: what the
// page asks of iOS itself. Its commands are allowed in capabilities/default.json, called through
// `api::native` of the page (src/api/native.rs in the repository root). Tauri calls them on a
// queue of its own, UIKit wants the main thread.
class HashiPlugin: Plugin {
  private weak var webview: WKWebView?

  /// The page's `games_sign_in`s waiting for GameKit's first answer, on the main thread.
  private var signInWaiting: [Invoke] = []
  private var signInStarted = false
  private var signInAnswered = false
  /// The sign-in sheet GameKit handed over and nobody saw, for `games_show`.
  private var signInSheet: UIViewController?
  /// Set once GameKit's sign-in sheet was shown by itself: once per install, never again.
  private static let promptedKey = "hashi.gameCenterPrompted"

  override func load(webview: WKWebView) {
    self.webview = webview
    // iOS has no back button: the swipe from the screen's edge goes back in the page's history,
    // the router's pages (a group, a player, the rules), as the back gesture does on Android
    webview.allowsBackForwardNavigationGestures = true
  }

  /**
   * The status bar's icons for the page's theme, not the phone's: light ones over a dark page,
   * dark ones over a light page. The status bar follows the window's style, which the share
   * sheet and the keyboard take on as well.
   */
  @objc public func systemBars(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(SystemBarsArgs.self)
    DispatchQueue.main.async {
      self.webview?.window?.overrideUserInterfaceStyle = args.dark ? .dark : .light
      self.manager.viewController?.setNeedsStatusBarAppearanceUpdate()
      invoke.resolve()
    }
  }

  /**
   * The system's share sheet: a text, or a text as a file (a puzzle's CNF, which the website
   * downloads), which the sheet offers to save in Files too. The `WebView` downloads no Blob
   * links. The file goes to the app's temporary folder, which iOS empties itself.
   */
  @objc public func share(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(ShareArgs.self)
    var item: Any = args.text
    if let file = args.file {
      // The name only, never a path out of the folder
      let name = (file as NSString).lastPathComponent
      guard !name.isEmpty, name != ".", name != ".." else {
        invoke.reject("not a file name: \(file)")
        return
      }
      let folder = FileManager.default.temporaryDirectory.appendingPathComponent("share", isDirectory: true)
      try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
      let url = folder.appendingPathComponent(name)
      try args.text.write(to: url, atomically: true, encoding: .utf8)
      item = url
    }
    DispatchQueue.main.async {
      guard var top = self.manager.viewController, let webview = self.webview else {
        invoke.reject("the app has no window")
        return
      }
      while let shown = top.presentedViewController {
        top = shown
      }
      let sheet = UIActivityViewController(activityItems: [item], applicationActivities: nil)
      // On the iPad the sheet is a popover, which needs a place to point at: the page's middle,
      // without an arrow (the page knows the button, iOS doesn't)
      if let popover = sheet.popoverPresentationController {
        popover.sourceView = webview
        popover.sourceRect = CGRect(x: webview.bounds.midX, y: webview.bounds.midY, width: 0, height: 0)
        popover.permittedArrowDirections = []
      }
      top.present(sheet, animated: true)
      invoke.resolve()
    }
  }

  /**
   * Game Center's sign-in, silent: GameKit signs in whoever is signed in on the phone and calls
   * the handler (again whenever the app comes back to the front). When nobody is, it hands over
   * its sign-in sheet, shown by itself once per install at most and never in the simulator (no
   * Apple Account there, and every test of the smoke test starts as a fresh install), else kept
   * for the button of the page (`gamesShow`). Answers once GameKit did, or after 10 s.
   */
  @objc public func gamesSignIn(_ invoke: Invoke) {
    DispatchQueue.main.async {
      if self.signInAnswered {
        invoke.resolve(self.gamesStatus())
        return
      }
      self.signInWaiting.append(invoke)
      guard !self.signInStarted else { return }
      self.signInStarted = true
      GKLocalPlayer.local.authenticateHandler = { [weak self] sheet, error in
        DispatchQueue.main.async {
          guard let self else { return }
          if let error {
            Logger.info("Game Center: \(error.localizedDescription)")
          }
          self.signInSheet = sheet
          if let sheet, self.mayPrompt() {
            UserDefaults.standard.set(true, forKey: HashiPlugin.promptedKey)
            self.signInSheet = nil
            self.present(sheet)
          }
          self.answerSignIn()
        }
      }
      DispatchQueue.main.asyncAfter(deadline: .now() + 10) { self.answerSignIn() }
    }
  }

  /**
   * What the server says the player has: an achievement's percentComplete (of its steps, 100
   * once earned) and the scores, while a player is signed in. Only achievements that got further
   * than Game Center has them; its completion banner only when one of them is done now, not for
   * a row of them at once (the stamps from before the app had Game Center).
   */
  @objc public func gamesReport(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(GamesReportArgs.self)
    DispatchQueue.main.async {
      guard GKLocalPlayer.local.isAuthenticated else {
        invoke.resolve()
        return
      }
      GKAchievement.loadAchievements { known, error in
        if let error {
          Logger.info("Game Center: no achievements: \(error.localizedDescription)")
        }
        var before: [String: Double] = [:]
        for achievement in known ?? [] {
          before[achievement.identifier] = max(before[achievement.identifier] ?? 0, achievement.percentComplete)
        }
        let achievements = args.achievements
          .filter { $0.percent > (before[$0.id] ?? 0) }
          .map { reported -> GKAchievement in
            let achievement = GKAchievement(identifier: reported.id)
            achievement.percentComplete = reported.percent
            achievement.showsCompletionBanner = false
            return achievement
          }
        let completed = achievements.filter { $0.percentComplete >= 100 }
        if completed.count == 1 && error == nil {
          completed[0].showsCompletionBanner = true
        }
        if !achievements.isEmpty {
          GKAchievement.report(achievements) { error in
            if let error {
              Logger.info("Game Center: achievements not reported: \(error.localizedDescription)")
            }
          }
        }
        for score in args.scores {
          GKLeaderboard.submitScore(
            Int(score.value), context: 0, player: GKLocalPlayer.local, leaderboardIDs: [score.id]
          ) { error in
            if let error {
              Logger.info("Game Center: score not submitted: \(error.localizedDescription)")
            }
          }
        }
        invoke.resolve()
      }
    }
  }

  /**
   * Game Center's own screens, the achievements or the leaderboards, through the access point
   * (its floating button stays off): GKGameCenterViewController is deprecated since iOS 26, and
   * the access point presents and dismisses them itself. A player who isn't signed in gets
   * GameKit's sign-in sheet instead, if it handed one over.
   */
  @objc public func gamesShow(_ invoke: Invoke) throws {
    let args = try invoke.parseArgs(GamesShowArgs.self)
    let state: GKGameCenterViewControllerState
    switch args.kind {
    case "achievements": state = .achievements
    case "leaderboards": state = .leaderboards
    default:
      invoke.reject("no such screen: \(args.kind)")
      return
    }
    DispatchQueue.main.async {
      if GKLocalPlayer.local.isAuthenticated {
        GKAccessPoint.shared.trigger(state: state) {}
        invoke.resolve()
      } else if let sheet = self.signInSheet {
        self.signInSheet = nil
        self.present(sheet)
        invoke.resolve()
      } else {
        invoke.reject("not signed in to Game Center")
      }
    }
  }

  /// Answers the `games_sign_in`s waiting, on the main thread.
  private func answerSignIn() {
    signInAnswered = true
    let waiting = signInWaiting
    signInWaiting = []
    for invoke in waiting {
      invoke.resolve(gamesStatus())
    }
  }

  private func gamesStatus() -> GamesStatus {
    GamesStatus(available: true, signedIn: GKLocalPlayer.local.isAuthenticated)
  }

  /// Whether GameKit's sign-in sheet may show by itself (`gamesSignIn`).
  private func mayPrompt() -> Bool {
    #if targetEnvironment(simulator)
      return false
    #else
      return !UserDefaults.standard.bool(forKey: HashiPlugin.promptedKey)
    #endif
  }

  /// Shows `sheet` over whatever the app shows, on the main thread.
  private func present(_ sheet: UIViewController) {
    guard var top = manager.viewController else { return }
    while let shown = top.presentedViewController {
      top = shown
    }
    top.present(sheet, animated: true)
  }
}

@_cdecl("init_plugin_hashi")
public func initPlugin() -> Plugin {
  return HashiPlugin()
}
