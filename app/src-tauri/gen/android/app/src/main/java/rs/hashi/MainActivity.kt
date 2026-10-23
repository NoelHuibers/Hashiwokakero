package rs.hashi

import android.graphics.Color
import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.graphics.Insets
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.webkit.ScriptHandler
import androidx.webkit.WebViewCompat
import androidx.webkit.WebViewFeature

// The page draws behind the status and the navigation bar (edge to edge, which Android 15
// enforces for apps targeting it). The WebView doesn't tell the page where the bars are
// (env(safe-area-inset-*) stays 0 in WebView 124), so this activity does: their sizes go into
// the CSS variables --app-inset-top, -right, -bottom and -left of the page (in CSS pixels), which
// style/tailwind.css turns into the header's and the tab bar's padding. Their icons follow the
// page's theme, which only the page knows (HashiPlugin.systemBars).
class MainActivity : TauriActivity() {
  /** The script that sets the variables in every new page, replaced when the bars change. */
  private var insetsScript: ScriptHandler? = null
  private var shownInsets: String? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  override fun onWebViewCreate(webView: WebView) {
    // Until the page paints, the window's background shows (the theme's, light or dark like the
    // phone) instead of a white WebView
    webView.setBackgroundColor(Color.TRANSPARENT)
    ViewCompat.setOnApplyWindowInsetsListener(webView) { view, insets ->
      val bars = insets.getInsets(
        WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout(),
      )
      showInsets(view as WebView, bars)
      // The WebView's own handling as well, for WebViews that report the insets themselves
      ViewCompat.onApplyWindowInsets(view, insets)
    }
    ViewCompat.requestApplyInsets(webView)
  }

  /** Light icons on the bars over a dark page, dark ones over a light page. */
  fun systemBars(dark: Boolean) = runOnUiThread {
    WindowCompat.getInsetsController(window, window.decorView).run {
      isAppearanceLightStatusBars = !dark
      isAppearanceLightNavigationBars = !dark
    }
  }

  /**
   * Sets the variables in the page shown now and, as a script at the start of every document,
   * in every page loaded later (the app's pages are one document, but a reload makes a new one).
   */
  private fun showInsets(webView: WebView, bars: Insets) {
    val density = resources.displayMetrics.density
    val sides = listOf("top" to bars.top, "right" to bars.right, "bottom" to bars.bottom, "left" to bars.left)
    val values = sides.joinToString("") { (side, px) -> "s.setProperty('--app-inset-$side','${px / density}px');" }
    val script = "(function(){var e=document.documentElement;if(!e)return;var s=e.style;$values})()"
    if (script == shownInsets) return
    shownInsets = script
    if (WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
      insetsScript?.remove()
      insetsScript = WebViewCompat.addDocumentStartJavaScript(webView, script, setOf("*"))
    }
    webView.evaluateJavascript(script, null)
  }
}
