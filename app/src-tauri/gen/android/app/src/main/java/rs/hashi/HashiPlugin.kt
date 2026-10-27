package rs.hashi

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.util.Log
import androidx.activity.result.ActivityResult
import androidx.core.content.FileProvider
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.google.android.gms.games.PlayGames
import com.google.android.gms.tasks.Task
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
   * Whether this build has Google Play Games: a project id (build.gradle.kts, playGames), with
   * which the SDK started itself when the app did (its provider in AndroidManifest.xml) and
   * signed the player in if it could (automatic sign-in). Without one none of the games_
   * commands touches the SDK, which never started.
   */
  private val games = BuildConfig.PLAY_GAMES_APP_ID.isNotEmpty()

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
    if (!games) {
      invoke.resolve(gamesStatus(false))
      return
    }
    signedIn { invoke.resolve(gamesStatus(it)) }
  }

  /**
   * What the server says the player has: a counting achievement's steps (Play unlocks it at its
   * total), another one unlocked, the scores. Play keeps the most steps and the best score and
   * sends them when it can, so a report again changes nothing. Skipped while nobody is signed in.
   */
  @Command
  fun gamesReport(invoke: Invoke) {
    try {
      val args = invoke.parseArgs(GamesReportArgs::class.java)
      if (!games) {
        invoke.resolve()
        return
      }
      signedIn { signedIn ->
        if (signedIn) {
          val achievements = PlayGames.getAchievementsClient(activity)
          for (achievement in args.achievements) {
            val steps = achievement.steps
            if (steps != null && steps > 0) {
              achievements.setSteps(achievement.id, steps)
            } else if (achievement.earned) {
              achievements.unlock(achievement.id)
            }
          }
          val leaderboards = PlayGames.getLeaderboardsClient(activity)
          for (score in args.scores) {
            leaderboards.submitScore(score.id, score.value)
          }
        }
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
    if (!games) {
      invoke.reject("this build has no Play Games")
      return
    }
    if (kind != "achievements" && kind != "leaderboards") {
      invoke.reject("no such screen: $kind")
      return
    }
    val client = PlayGames.getGamesSignInClient(activity)
    client.isAuthenticated().continueWithTask { task ->
      if (task.isSuccessful && task.result.isAuthenticated) task else client.signIn()
    }.addOnCompleteListener { task ->
      if (!task.isSuccessful || !task.result.isAuthenticated) {
        invoke.reject("not signed in to Play Games")
        return@addOnCompleteListener
      }
      val intent: Task<Intent> = if (kind == "leaderboards") {
        PlayGames.getLeaderboardsClient(activity).allLeaderboardsIntent
      } else {
        PlayGames.getAchievementsClient(activity).achievementsIntent
      }
      intent
        .addOnSuccessListener { startActivityForResult(invoke, it, "gamesShown") }
        .addOnFailureListener { invoke.reject(it.message) }
    }
  }

  @ActivityCallback
  private fun gamesShown(invoke: Invoke, result: ActivityResult) {
    invoke.resolve()
  }

  /** The automatic sign-in's result, false if it failed. */
  private fun signedIn(then: (Boolean) -> Unit) {
    PlayGames.getGamesSignInClient(activity).isAuthenticated().addOnCompleteListener { task ->
      if (!task.isSuccessful) Log.w("HashiPlugin", "Play Games sign-in failed", task.exception)
      then(task.isSuccessful && task.result.isAuthenticated)
    }
  }

  private fun gamesStatus(signedIn: Boolean) = JSObject().apply {
    put("available", games)
    put("signedIn", signedIn)
  }
}
