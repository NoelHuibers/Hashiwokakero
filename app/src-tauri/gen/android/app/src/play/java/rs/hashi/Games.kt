package rs.hashi

import android.app.Activity
import android.content.Intent
import android.util.Log
import com.google.android.gms.games.PlayGames
import com.google.android.gms.tasks.Task

/**
 * Google Play Games for HashiPlugin's games_ commands, in the Play build (app/README.md, Android
 * builds). The foss build has its own Games without the SDK (src/foss).
 */
internal class Games(private val activity: Activity) {
  /**
   * Whether this build has Google Play Games: a project id (build.gradle.kts, playGames), with
   * which the SDK started itself when the app did (its provider in src/play/AndroidManifest.xml)
   * and signed the player in if it could (automatic sign-in). Without one HashiPlugin never calls
   * the other functions, and the SDK never started.
   */
  val available = BuildConfig.PLAY_GAMES_APP_ID.isNotEmpty()

  /** The automatic sign-in's result, false if it failed. */
  fun signedIn(then: (Boolean) -> Unit) {
    PlayGames.getGamesSignInClient(activity).isAuthenticated().addOnCompleteListener { task ->
      if (!task.isSuccessful) Log.w("HashiPlugin", "Play Games sign-in failed", task.exception)
      then(task.isSuccessful && task.result.isAuthenticated)
    }
  }

  /**
   * A counting achievement's steps (Play unlocks it at its total), another one unlocked, the
   * scores. Play keeps the most steps and the best score and sends them when it can. Only for a
   * signed-in player.
   */
  fun report(achievements: Array<GamesAchievement>, scores: Array<GamesScore>) {
    val client = PlayGames.getAchievementsClient(activity)
    for (achievement in achievements) {
      val steps = achievement.steps
      if (steps != null && steps > 0) {
        client.setSteps(achievement.id, steps)
      } else if (achievement.earned) {
        client.unlock(achievement.id)
      }
    }
    val leaderboards = PlayGames.getLeaderboardsClient(activity)
    for (score in scores) {
      leaderboards.submitScore(score.id, score.value)
    }
  }

  /**
   * The screen of the achievements or the leaderboards ([kind]), signing the player in first if
   * nobody is: [start] gets its intent, [fail] the reason when there is none.
   */
  fun show(kind: String, start: (Intent) -> Unit, fail: (String?) -> Unit) {
    val client = PlayGames.getGamesSignInClient(activity)
    client.isAuthenticated().continueWithTask { task ->
      if (task.isSuccessful && task.result.isAuthenticated) task else client.signIn()
    }.addOnCompleteListener { task ->
      if (!task.isSuccessful || !task.result.isAuthenticated) {
        fail("not signed in to Play Games")
        return@addOnCompleteListener
      }
      val intent: Task<Intent> = if (kind == "leaderboards") {
        PlayGames.getLeaderboardsClient(activity).allLeaderboardsIntent
      } else {
        PlayGames.getAchievementsClient(activity).achievementsIntent
      }
      intent
        .addOnSuccessListener { start(it) }
        .addOnFailureListener { fail(it.message) }
    }
  }
}
