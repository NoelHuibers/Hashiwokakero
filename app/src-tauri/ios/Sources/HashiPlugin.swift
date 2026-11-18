import GameKit
import Tauri
import UIKit
import UserNotifications
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

/// The page's `NotifyState` (src/api/native.rs): whether iOS lets the app show notifications.
struct NotifyStatus: Encodable {
  let granted: Bool
}

/// The page's `PushToken`: the device token for Apple's push service, and which service it is for.
struct PushToken: Encodable {
  let token: String
  let environment: String
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
  /// The page's `games_show` that showed that sheet, answered once GameKit signed the player in.
  private var showWaiting: Invoke?
  /// Set once GameKit's sign-in sheet was shown by itself: once per install, never again.
  private static let promptedKey = "hashi.gameCenterPrompted"

  /// The page's `notify_token`s waiting for iOS's answer, on the main thread, and the round they
  /// belong to (a late timeout doesn't answer the next round's).
  private var tokenWaiting: [Invoke] = []
  private var tokenRound = 0
  /// How long iOS gets for a device token.
  private static let tokenTimeout: TimeInterval = 30

  override func load(webview: WKWebView) {
    self.webview = webview
    // iOS has no back button: the swipe from the screen's edge goes back in the page's history,
    // the router's pages (a group, a player, the rules), as the back gesture does on Android
    webview.allowsBackForwardNavigationGestures = true
    // iOS hands the device token to the app's delegate, which main.mm passes on as these
    let center = NotificationCenter.default
    center.addObserver(forName: Notification.Name("HashiDeviceToken"), object: nil, queue: .main) {
      [weak self] note in
      self?.answerToken(note.object as? Data, error: nil)
    }
    center.addObserver(forName: Notification.Name("HashiDeviceTokenFailed"), object: nil, queue: .main) {
      [weak self] note in
      let error = (note.object as? NSError)?.localizedDescription ?? "unknown error"
      self?.answerToken(nil, error: "iOS gave no device token: \(error)")
    }
  }

  /**
   * Whether the app may show notifications, asking the player if iOS hasn't asked yet (once per
   * install: after a no only iOS's settings change it). Alerts and sounds, no badge.
   */
  @objc public func notifyPermission(_ invoke: Invoke) {
    UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { granted, error in
      if let error {
        Logger.info("notifications: \(error.localizedDescription)")
      }
      invoke.resolve(NotifyStatus(granted: granted))
    }
  }

  /// Whether the app may show notifications, without asking.
  @objc public func notifyState(_ invoke: Invoke) {
    UNUserNotificationCenter.current().getNotificationSettings { settings in
      invoke.resolve(NotifyStatus(granted: HashiPlugin.allows(settings.authorizationStatus)))
    }
  }

  /**
   * The device token for Apple's push service (hex) and the service it is for, which the page
   * registers with the server: `registerForRemoteNotifications`, whose answer comes to the app's
   * delegate (main.mm). iOS gives the same token every time until it gives the app a new one
   * (after a restore, a reinstall), so the page asks at every start. Refused without an answer
   * within 30 s.
   */
  @objc public func notifyToken(_ invoke: Invoke) {
    DispatchQueue.main.async {
      self.tokenWaiting.append(invoke)
      guard self.tokenWaiting.count == 1 else { return }
      let round = self.tokenRound
      UIApplication.shared.registerForRemoteNotifications()
      DispatchQueue.main.asyncAfter(deadline: .now() + HashiPlugin.tokenTimeout) {
        if round == self.tokenRound {
          self.answerToken(nil, error: "iOS gave no device token within 30 s")
        }
      }
    }
  }

  /// Answers the `notify_token`s waiting, on the main thread.
  private func answerToken(_ token: Data?, error: String?) {
    let waiting = tokenWaiting
    tokenWaiting = []
    tokenRound += 1
    for invoke in waiting {
      if let token {
        let hex = token.map { String(format: "%02x", $0) }.joined()
        invoke.resolve(PushToken(token: hex, environment: HashiPlugin.apsEnvironment))
      } else {
        invoke.reject(error ?? "iOS gave no device token")
      }
    }
  }

  private static func allows(_ status: UNAuthorizationStatus) -> Bool {
    switch status {
    case .authorized, .provisional, .ephemeral: return true
    default: return false
    }
  }

  /**
   * The app's `aps-environment`, which says whose tokens iOS gives it: the sandbox's for a build
   * signed for development, production's for the App Store and TestFlight. A signed build has it
   * in its provisioning profile, which the App Store's and TestFlight's installs don't have
   * (production); the simulator's tokens are the sandbox's.
   */
  static let apsEnvironment: String = {
    #if targetEnvironment(simulator)
      return "development"
    #else
      guard
        let url = Bundle.main.url(forResource: "embedded", withExtension: "mobileprovision"),
        let data = try? Data(contentsOf: url),
        let start = data.range(of: Data("<?xml".utf8)),
        let end = data.range(of: Data("</plist>".utf8), in: start.lowerBound..<data.endIndex),
        let plist = try? PropertyListSerialization.propertyList(
          from: data.subdata(in: start.lowerBound..<end.upperBound), format: nil) as? [String: Any],
        let entitlements = plist["Entitlements"] as? [String: Any],
        let environment = entitlements["aps-environment"] as? String
      else { return "production" }
      return environment
    #endif
  }()

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
          if GKLocalPlayer.local.isAuthenticated, let shown = self.showWaiting {
            self.showWaiting = nil
            shown.resolve()
          }
        }
      }
      DispatchQueue.main.asyncAfter(deadline: .now() + 10) { self.answerSignIn() }
    }
  }

  /**
   * What the server says the player has: an achievement's percentComplete (of its steps, 100
   * once earned) and the scores, while a player is signed in. Only achievements that got further
   * than Game Center has them; its completion banner only when one of them is done now, not for
   * a row of them at once (the stamps from before the app had Game Center). Answers once Game
   * Center took all of it, refused if it didn't take something (offline): the page sends that
   * again with its next report.
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
        // GameKit's answers, on the main thread
        let done = DispatchGroup()
        var failures: [String] = []
        if !achievements.isEmpty {
          done.enter()
          GKAchievement.report(achievements) { error in
            DispatchQueue.main.async {
              if let error {
                Logger.info("Game Center: achievements not reported: \(error.localizedDescription)")
                failures.append("achievements not reported: \(error.localizedDescription)")
              }
              done.leave()
            }
          }
        }
        for score in args.scores {
          done.enter()
          GKLeaderboard.submitScore(
            Int(score.value), context: 0, player: GKLocalPlayer.local, leaderboardIDs: [score.id]
          ) { error in
            DispatchQueue.main.async {
              if let error {
                Logger.info("Game Center: score not submitted: \(error.localizedDescription)")
                failures.append("\(score.id) not submitted: \(error.localizedDescription)")
              }
              done.leave()
            }
          }
        }
        done.notify(queue: .main) {
          if failures.isEmpty {
            invoke.resolve()
          } else {
            invoke.reject("Game Center: " + failures.joined(separator: "; "))
          }
        }
      }
    }
  }

  /**
   * Game Center's own screens, the achievements or the leaderboards, through the access point
   * (its floating button stays off): GKGameCenterViewController is deprecated since iOS 26, and
   * the access point presents and dismisses them itself. A player who isn't signed in gets
   * GameKit's sign-in sheet instead, if it handed one over: answered once GameKit signed the
   * player in on it (it calls the sign-in's handler again), so that the page reports the stamps
   * then, never if the sheet is cancelled.
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
        self.showWaiting = invoke
        self.present(sheet)
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
