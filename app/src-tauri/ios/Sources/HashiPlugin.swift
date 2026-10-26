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

// The app's own plugin ("hashi" in src/lib.rs) on iOS, HashiPlugin.kt's counterpart: what the
// page asks of iOS itself. Its commands are allowed in capabilities/default.json, called through
// `api::native` of the page (src/api/native.rs in the repository root). Tauri calls them on a
// queue of its own, UIKit wants the main thread.
class HashiPlugin: Plugin {
  private weak var webview: WKWebView?

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
}

@_cdecl("init_plugin_hashi")
public func initPlugin() -> Plugin {
  return HashiPlugin()
}
