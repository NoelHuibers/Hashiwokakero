package rs.hashi

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import androidx.activity.result.ActivityResult
import androidx.core.content.FileProvider
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
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

/** An achievement as the page reports it (src/api/games.rs): steps of a target, or unlocked. */
@InvokeArg
class GamesAchievement {
  lateinit var id: String
  var steps: Int? = null
  var target: Int? = null
  var earned: Boolean = false
}

@InvokeArg
class GamesScore {
  lateinit var id: String
  var value: Long = 0
}

// Arrays, not lists: R8 drops the generic signatures that Jackson would need for a List's items
@InvokeArg
class GamesReportArgs {
  var achievements: Array<GamesAchievement> = arrayOf()
  var scores: Array<GamesScore> = arrayOf()
}

@InvokeArg
class GamesShowArgs {
  /** "achievements" or "leaderboards". */
  var kind: String = "achievements"
}

// The app's own plugin ("hashi" in src-tauri/src/lib.rs): what the page asks of Android itself.
// Its commands are allowed in capabilities/default.json, called through `api::native` of the
// page (src/api/native.rs in the repository root).
@TauriPlugin
class HashiPlugin(private val activity: Activity) : Plugin(activity) {
  /**
   * Google Play Games: Games.kt of src/play, the SDK, in the Play build; of src/foss, nothing, in
   * the foss build (app/README.md, Android builds). Without the service (`available`: the foss
   * build, or a Play build without a project id) none of the games_ commands calls it.
   */
  private val games = Games(activity)

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

  /**
   * Whether the player is signed in to Play Games: the automatic sign-in's result, which this
   * waits for. Never asks; the page only learns whether to report (`available`).
   */
  @Command
  fun gamesSignIn(invoke: Invoke) {
    if (!games.available) {
      invoke.resolve(gamesStatus(false))
      return
    }
    games.signedIn { invoke.resolve(gamesStatus(it)) }
  }

  /**
   * What the server says the player has (Games.report). Play keeps the most steps and the best
   * score, so a report again changes nothing. Skipped while nobody is signed in.
   */
  @Command
  fun gamesReport(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(GamesReportArgs::class.java)
      if (!games.available) {
        invoke.resolve()
        return
      }
      games.signedIn { signedIn ->
        if (signedIn) games.report(args.achievements, args.scores)
        invoke.resolve()
      }
    } catch (ex: Exception) {
      invoke.reject(ex.message)
    }
  }

  /**
   * Play Games' screen of the game's achievements or leaderboards. A tap on the page's button:
   * the one time the plugin may ask the player to sign in first.
   */
  @Command
  fun gamesShow(invoke: Invoke) {
    val kind = invoke.parseArgs(GamesShowArgs::class.java).kind
    if (!games.available) {
      invoke.reject("this build has no Play Games")
      return
    }
    if (kind != "achievements" && kind != "leaderboards") {
      invoke.reject("no such screen: $kind")
      return
    }
    games.show(
      kind,
      { startActivityForResult(invoke, it, "gamesShown") },
      { invoke.reject(it) },
    )
  }

  @ActivityCallback
  private fun gamesShown(invoke: Invoke, result: ActivityResult) {
    invoke.resolve()
  }

  private fun gamesStatus(signedIn: Boolean) = JSObject().apply {
    put("available", games.available)
    put("signedIn", signedIn)
  }
}
