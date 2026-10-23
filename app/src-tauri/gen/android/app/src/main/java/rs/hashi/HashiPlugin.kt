package rs.hashi

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import androidx.core.content.FileProvider
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.Plugin
import java.io.File

@InvokeArg
class SystemBarsArgs {
  var dark: Boolean = false
}

@InvokeArg
class ShareArgs {
  lateinit var text: String
  /** A file name: shares `text` as this file instead of as text. */
  var file: String? = null
  var mime: String = "text/plain"
}

// The app's own plugin ("hashi" in src-tauri/src/lib.rs): what the page asks of Android itself.
// Its commands are allowed in capabilities/default.json, called through `api::native` of the
// page (src/api/native.rs in the repository root).
@TauriPlugin
class HashiPlugin(private val activity: Activity) : Plugin(activity) {
  /** The icons of the system bars for the page's theme, see MainActivity. */
  @Command
  fun systemBars(invoke: Invoke) {
    val args = invoke.parseArgs(SystemBarsArgs::class.java)
    (activity as MainActivity).systemBars(args.dark)
    invoke.resolve()
  }

  /**
   * The system's share sheet, which the WebView doesn't open (no navigator.share): a text, or a
   * text as a file (a puzzle's CNF, which the website downloads). The file goes to the app's
   * cache, where the FileProvider of AndroidManifest.xml lends it to the app it's shared with.
   */
  @Command
  fun share(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(ShareArgs::class.java)
      val send = Intent(Intent.ACTION_SEND).setType(args.mime)
      val name = args.file
      if (name == null) {
        send.putExtra(Intent.EXTRA_TEXT, args.text)
      } else {
        // The name only, never a path out of the folder
        val file = File(File(activity.cacheDir, "share").apply { mkdirs() }, File(name).name)
        file.writeText(args.text)
        val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.fileprovider", file)
        send.putExtra(Intent.EXTRA_STREAM, uri)
        send.clipData = ClipData.newRawUri(file.name, uri)
        send.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
      }
      activity.startActivity(Intent.createChooser(send, null))
      invoke.resolve()
    } catch (ex: Exception) {
      invoke.reject(ex.message)
    }
  }
}
