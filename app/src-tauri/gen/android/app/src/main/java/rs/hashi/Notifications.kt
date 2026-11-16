package rs.hashi

import android.Manifest
import android.annotation.SuppressLint
import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequest
import androidx.work.PeriodicWorkRequest
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.TimeUnit
import kotlin.concurrent.thread
import org.json.JSONArray
import org.json.JSONObject
import org.json.JSONTokener

/**
 * Notifications without a push service (docs/notifications.md, "Android app"): the app asks our
 * server itself. No Firebase, no Google Play services, the same in the foss build.
 *
 * - The page registers the phone with the server (`register_notification_device`) and hands its
 *   id and secret here ([start], HashiPlugin's `notify_start`), which keeps them in the app's own
 *   preferences (not in the backups, see res/xml/backup_rules.xml).
 * - [PollWorker] asks the server every 15 minutes (WorkManager's shortest period, with a network)
 *   for the new notifications (`poll_notifications`, a server function called like the page calls
 *   it: a POST of JSON to `/api/poll_notifications`) and shows them, a channel per kind.
 * - The streak's last call comes at a set time, which the server says with every answer
 *   (`next_last_call`). WorkManager's periods stretch to hours while the phone sleeps (Doze), so
 *   an inexact alarm that may go off in Doze asks once more around then ([LastCallReceiver]).
 *   No exact alarm: a few minutes late is fine, and it needs no permission.
 * - An answer `null` means the server forgot the phone (the account deleted, the settings off on
 *   another device's sign-out...): everything stops here too. Turning notifications off or
 *   signing out ([stop]) stops it as well and tells the server ([ForgetWorker]).
 */
// WorkManager's builders, not its Kotlin functions: those are inlined from JVM 11 bytecode, the
// app's is 1.8
object Notifications {
  private const val TAG = "HashiNotify"
  private const val PREFS = "hashi.notifications"
  private const val POLL = "hashi.notifications.poll"
  private const val POLL_NOW = "hashi.notifications.poll-now"
  private const val FORGET = "hashi.notifications.forget"
  const val ACTION_LAST_CALL = "rs.hashi.action.LAST_CALL"

  /** The kinds of the server (`NotifyKind::id`), each a channel of its own; others go to "other". */
  private val KINDS = listOf("last_call", "race", "rival", "shared", "group")
  private const val OTHER = "other"

  /** The server's clock and the phone's may differ a little: the alarm goes this much later. */
  private const val MARGIN_MS = 60_000L
  /** An alarm this close to the last call goes off for it, see [alarmAt]. */
  private const val CLOSE_MS = 10 * 60_000L

  /** The phone as the server knows it. */
  data class Device(val server: String, val app: String, val id: Long, val secret: String)

  /** How a poll went. */
  enum class Poll { DONE, GONE, FAILED }

  private fun prefs(context: Context) =
    context.applicationContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

  fun device(context: Context): Device? {
    val p = prefs(context)
    val id = p.getLong("device", 0)
    val secret = p.getString("secret", null)
    val server = p.getString("server", null)
    if (id == 0L || secret == null || server == null) return null
    return Device(server, p.getString("app", "") ?: "", id, secret)
  }

  /** Keeps the phone's id and secret and starts asking: now, and every 15 minutes. */
  fun start(context: Context, device: Device) {
    val old = device(context)
    if (old != null && (old.id != device.id || old.server != device.server)) leave(context, old)
    prefs(context).edit()
      .putString("server", device.server.trimEnd('/'))
      .putString("app", device.app)
      .putLong("device", device.id)
      .putString("secret", device.secret)
      .apply()
    channels(context)
    val online = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()
    val work = WorkManager.getInstance(context)
    work.enqueueUniquePeriodicWork(
      POLL,
      ExistingPeriodicWorkPolicy.UPDATE,
      PeriodicWorkRequest.Builder(PollWorker::class.java, 15, TimeUnit.MINUTES).setConstraints(online).build(),
    )
    pollSoon(context)
  }

  /** Stops asking, forgets the phone here and tells the server (also later, offline now). */
  fun stop(context: Context) {
    val device = device(context)
    clear(context)
    if (device != null) leave(context, device)
  }

  /** Stops everything without telling the server: it forgot the phone, or [stop]. */
  private fun clear(context: Context) {
    prefs(context).edit()
      .remove("server").remove("app").remove("device").remove("secret").remove("last_call")
      .apply()
    WorkManager.getInstance(context).cancelUniqueWork(POLL)
    WorkManager.getInstance(context).cancelUniqueWork(POLL_NOW)
    alarms(context).cancel(alarm(context))
  }

  /** The server is to forget `device`: [ForgetWorker] tells it, once the phone is online. */
  private fun leave(context: Context, device: Device) {
    prefs(context).edit()
      .putString("forget", JSONObject()
        .put("server", device.server).put("app", device.app)
        .put("id", device.id).put("secret", device.secret).toString())
      .apply()
    val online = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()
    WorkManager.getInstance(context).enqueueUniqueWork(
      FORGET,
      ExistingWorkPolicy.REPLACE,
      OneTimeWorkRequest.Builder(ForgetWorker::class.java).setConstraints(online).build(),
    )
  }

  /** One poll as soon as the phone is online: after [start], or an alarm's poll that failed. */
  fun pollSoon(context: Context) {
    val online = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()
    WorkManager.getInstance(context).enqueueUniqueWork(
      POLL_NOW,
      ExistingWorkPolicy.REPLACE,
      OneTimeWorkRequest.Builder(PollWorker::class.java).setConstraints(online).build(),
    )
  }

  /** Asks the server for the new notifications and shows them. */
  fun poll(context: Context): Poll {
    val device = device(context) ?: run {
      clear(context)
      return Poll.GONE
    }
    val answer = try {
      call(device, "poll_notifications", JSONObject().put("device", device.id).put("secret", device.secret))
    } catch (ex: IOException) {
      Log.i(TAG, "poll failed: $ex")
      null
    }
    if (answer == null) return Poll.FAILED
    return try {
      val value = JSONTokener(answer).nextValue()
      if (value !is JSONObject) {
        // `null`: the server forgot the phone
        Log.i(TAG, "the server forgot this device")
        clear(context)
        return Poll.GONE
      }
      val messages = value.optJSONArray("messages") ?: JSONArray()
      for (i in 0 until messages.length()) show(context, messages.getJSONObject(i))
      lastCall(context, if (value.isNull("next_last_call")) null else value.getLong("next_last_call"))
      Poll.DONE
    } catch (ex: Exception) {
      Log.w(TAG, "poll answered something else: $ex")
      Poll.FAILED
    }
  }

  /**
   * A server function: a POST of its arguments as JSON to `/api/<name>`, with the app's
   * `x-hashi-app` header like the page's calls. The answer's text, `null` for an error the server
   * answered (another status than 200). Fails with an IOException without a connection.
   */
  private fun call(device: Device, name: String, args: JSONObject): String? {
    val connection = URL("${device.server}/api/$name").openConnection() as HttpURLConnection
    try {
      connection.requestMethod = "POST"
      connection.connectTimeout = 15_000
      connection.readTimeout = 15_000
      connection.doOutput = true
      connection.setRequestProperty("Content-Type", "application/json")
      connection.setRequestProperty("Accept", "application/json")
      if (device.app.isNotEmpty()) connection.setRequestProperty("x-hashi-app", device.app)
      connection.outputStream.use { it.write(args.toString().toByteArray()) }
      val status = connection.responseCode
      if (status != HttpURLConnection.HTTP_OK) {
        Log.w(TAG, "$name: HTTP $status")
        return null
      }
      return connection.inputStream.bufferedReader().use { it.readText() }
    } finally {
      connection.disconnect()
    }
  }

  /** Tells the server to forget the phone [leave] noted. Whether that's done (or hopeless). */
  fun forget(context: Context): Boolean {
    val noted = prefs(context).getString("forget", null) ?: return true
    val done = try {
      val json = JSONObject(noted)
      val device = Device(json.getString("server"), json.optString("app"), json.getLong("id"), json.getString("secret"))
      call(device, "unregister_notification_device", JSONObject().put("id", device.id).put("secret", device.secret))
      true
    } catch (ex: IOException) {
      false
    } catch (ex: Exception) {
      true
    }
    if (done) prefs(context).edit().remove("forget").apply()
    return done
  }

  // --- Showing them ---

  /**
   * A channel per kind, named like the account page's switches (res/values-*, written by a test
   * of the server: src/server/notify/render.rs), so players can tune them in Android's settings
   * too. Again with every start and poll: Android only renames them, in the phone's language.
   */
  fun channels(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val manager = context.getSystemService(NotificationManager::class.java) ?: return
    val res = context.resources
    val names = mapOf(
      "last_call" to (R.string.notify_last_call to R.string.notify_last_call_hint),
      "race" to (R.string.notify_race to R.string.notify_race_hint),
      "rival" to (R.string.notify_rival to R.string.notify_rival_hint),
      "shared" to (R.string.notify_shared to R.string.notify_shared_hint),
      "group" to (R.string.notify_group to R.string.notify_group_hint),
    )
    val channels = KINDS.map { kind ->
      val (name, hint) = names.getValue(kind)
      NotificationChannel(kind, res.getString(name), NotificationManager.IMPORTANCE_DEFAULT)
        .apply { description = res.getString(hint) }
    } + NotificationChannel(OTHER, res.getString(R.string.notify_other), NotificationManager.IMPORTANCE_DEFAULT)
    manager.createNotificationChannels(channels)
  }

  /** Shows a `PushMessage` of the server, replacing the one of the same tag. */
  @SuppressLint("MissingPermission")
  private fun show(context: Context, message: JSONObject) {
    val now = System.currentTimeMillis()
    val expires = message.optLong("expires_at", Long.MAX_VALUE)
    if (expires <= now) return
    val notifications = NotificationManagerCompat.from(context)
    // Refused on Android 13 and newer, or turned off in the settings: nothing to show
    if (!notifications.areNotificationsEnabled()) return
    if (Build.VERSION.SDK_INT >= 33 &&
      ContextCompat.checkSelfPermission(context, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
    ) return
    channels(context)
    val kind = message.optString("kind").takeIf { it in KINDS } ?: OTHER
    val tag = message.optString("tag", message.optString("id"))
    val body = message.optString("body")
    val notification = NotificationCompat.Builder(context, kind)
      .setSmallIcon(R.drawable.ic_notification)
      .setColor(ContextCompat.getColor(context, R.color.primary))
      .setContentTitle(message.optString("title"))
      .setContentText(body)
      .setStyle(NotificationCompat.BigTextStyle().bigText(body))
      .setCategory(if (kind == "last_call") NotificationCompat.CATEGORY_REMINDER else NotificationCompat.CATEGORY_SOCIAL)
      .setContentIntent(tap(context, tag, message.optString("url")))
      .setAutoCancel(true)
      .apply { if (expires != Long.MAX_VALUE) setTimeoutAfter(expires - now) }
      .build()
    notifications.notify(tag, 0, notification)
  }

  /**
   * What a tap opens: the app at the message's page, an App Link of the site that goes to this
   * app (its package named, so also the debug build), which the deep-link plugin hands to the
   * page like a link from outside (`api::native::follow_app_links`). Anything else just opens the
   * app.
   */
  private fun tap(context: Context, tag: String, url: String): PendingIntent {
    val uri = Uri.parse(url)
    val intent = if (uri.scheme == "https" && uri.host == "hashi.rs") {
      Intent(Intent.ACTION_VIEW, uri).setPackage(context.packageName)
    } else {
      context.packageManager.getLaunchIntentForPackage(context.packageName) ?: Intent()
    }
    intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    return PendingIntent.getActivity(
      context,
      tag.hashCode(),
      intent,
      PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
    )
  }

  // --- The last call ---

  /**
   * When the alarm for a last call due at `due` goes off: at `due` once it is close, else halfway
   * there. An inexact alarm comes up to three quarters of its distance late (an hour at most), so
   * every step before the last goes off before `due`, and the last one, set within [CLOSE_MS],
   * at most about seven minutes after it.
   */
  fun alarmAt(now: Long, due: Long): Long = if (due - now <= CLOSE_MS) due else now + (due - now) / 2

  /** Notes when the server says the next last call is due, `null` for none, and sets the alarm. */
  private fun lastCall(context: Context, at: Long?) {
    if (at == null) {
      prefs(context).edit().remove("last_call").apply()
      alarms(context).cancel(alarm(context))
      return
    }
    val due = at + MARGIN_MS
    prefs(context).edit().putLong("last_call", due).apply()
    arm(context, due)
  }

  /** The alarm for the last call noted, again: after a restart of the phone, an update. */
  fun rearm(context: Context) {
    if (device(context) == null) return
    val due = prefs(context).getLong("last_call", 0)
    if (due > 0) arm(context, due)
  }

  /** The alarm went off: polls if the last call is due, else sets the next step. */
  fun alarmWent(context: Context, done: () -> Unit) {
    val due = prefs(context).getLong("last_call", 0)
    val now = System.currentTimeMillis()
    if (device(context) == null || due == 0L) return done()
    if (now < due - MARGIN_MS) {
      arm(context, due)
      return done()
    }
    // The alarm lets the app work and use the network for a few seconds, also in Doze
    thread(name = "hashi-last-call") {
      try {
        if (poll(context) == Poll.FAILED) pollSoon(context)
      } finally {
        done()
      }
    }
  }

  private fun arm(context: Context, due: Long) {
    val at = alarmAt(System.currentTimeMillis(), due)
    alarms(context).setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, at, alarm(context))
  }

  private fun alarms(context: Context) = context.getSystemService(AlarmManager::class.java)

  private fun alarm(context: Context): PendingIntent = PendingIntent.getBroadcast(
    context,
    0,
    Intent(context, LastCallReceiver::class.java).setAction(ACTION_LAST_CALL),
    PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
  )
}

/** Asks the server for the new notifications: every 15 minutes, and once after a start. */
class PollWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
  override fun doWork(): Result = when (Notifications.poll(applicationContext)) {
    Notifications.Poll.FAILED -> if (runAttemptCount < 3) Result.retry() else Result.success()
    else -> Result.success()
  }
}

/** Tells the server to forget the phone, once it is online. */
class ForgetWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
  override fun doWork(): Result =
    if (Notifications.forget(applicationContext) || runAttemptCount >= 5) Result.success() else Result.retry()
}

/**
 * The last call's alarm ([Notifications.alarmWent]), and the phone's restart and the app's update,
 * after which the alarm is set again (WorkManager sets its own work again by itself).
 */
class LastCallReceiver : BroadcastReceiver() {
  override fun onReceive(context: Context, intent: Intent) {
    when (intent.action) {
      Notifications.ACTION_LAST_CALL -> {
        val pending = goAsync()
        Notifications.alarmWent(context) { pending.finish() }
      }
      Intent.ACTION_BOOT_COMPLETED, Intent.ACTION_MY_PACKAGE_REPLACED -> Notifications.rearm(context)
    }
  }
}
