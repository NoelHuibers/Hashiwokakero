package rs.hashi

import android.app.Activity
import android.content.Intent

/**
 * The foss build's games service: none (app/README.md, Android builds). This build has no Google
 * Play Games, no SDK and no project id, so HashiPlugin answers as the Play build does without a
 * project id: `games_sign_in` says the service isn't there, `games_report` does nothing,
 * `games_show` refuses. The page then shows no Play Games button, as for a player without it.
 */
@Suppress("UNUSED_PARAMETER")
internal class Games(activity: Activity) {
  val available = false

  fun signedIn(then: (Boolean) -> Unit) = then(false)

  fun report(achievements: Array<GamesAchievement>, scores: Array<GamesScore>) {}

  fun show(kind: String, start: (Intent) -> Unit, fail: (String?) -> Unit) =
    fail("this build has no Play Games")
}
