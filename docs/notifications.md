# Notifications

Hashi tells players about five things, each with its own switch:

| Kind (`NotifyKind`) | When | Tap opens |
| --- | --- | --- |
| `last_call` | The player's streak is open (yesterday's daily solved and ranked or caught up, or a missed day after the streak that can still be caught up, see [streak saves](streak-saves.md)) and today's daily isn't done, about three hours before the daily changes, in their own time zone | `/` |
| `race` | Someone opens a race in one of the player's groups (not for rematches) | `/groups/<g>/races/<id>` |
| `rival` | A rival finishes today's daily faster than the player did (ranked results only) | `/` |
| `shared` | Players solve a puzzle the player shared, counted (`New solvers: 3`) | `/p/<code>` |
| `group` | A new group puzzle (kadai) is posted in one of the player's groups | `/groups/<g>/puzzles/<id>` |

The group river doesn't notify.

## Decisions

- **No Firebase, no FCM, no third-party notification service.** Everything is in our
  database and our server. The only services in between are the platforms' own push
  services: the browser vendor's for Web Push (with our own VAPID keys) and Apple's for the
  iOS app (APNs, with a p8 key). The Android app asks our server itself.
- **Per account, off by default.** One master switch turns everything on or off; a switch per
  kind under it. Turning the master on with nothing chosen turns every kind on; turning one
  kind on turns the master on; turning the last kind off turns it off
  (`model::NotificationPrefs`). Website and app edit the same settings, on the account page
  (`pages/account/notifications.rs`; the settings pop-up links there).
- **One offer.** After a finished daily the win card offers the last call once per account
  (`components/notify_offer.rs`, `model::should_offer`). It is noted as shown as soon as it
  shows; yes turns everything on.
- **Only where a device can get them.** `state::notify::can_deliver()` says whether this
  browser or app has a way of delivery (`delivers(DeviceKind)`): in browsers once the site has
  its Web Push keys and the browser can take them (below), in the Android app always
  ([The Android app](#the-android-app)), in the iOS app always ([The iOS app](#the-ios-app-apns)).
  Where it's `false` the offer never shows and the settings say that notifications can't reach
  this device yet. The switches still work: they are account settings.
- **Texts in the device's language.** The outbox keeps what a text needs (`notify::Args`,
  JSON), not the text: each device renders it in its own locale
  (`server/notify/render.rs`, keys `account.notifications.push.*` in every language). The tap
  target is a full `https://hashi.rs/<lang>/...` URL; the apps open those as app links.
- **The last call.** The daily changes at midnight UTC. Three hours before that is fine in
  most places (22:00 in Berlin's winter, 16:00 in New York, 13:00 in Los Angeles). If it is in
  the quiet hours (23:00–08:00 local), it comes at 21:00 of the evening before the change
  instead: Tokyo (change at 09:00) at 21:00, twelve hours before; Berlin in summer (change at
  02:00, three hours before is 23:00) at 21:00, five hours before. The time zone is the IANA
  name the browser or app reports (`Intl.DateTimeFormat().resolvedOptions().timeZone`), with
  daylight saving rules from the time zone database bundled with `jiff`
  (`server/notify/timing.rs`). Without one it's UTC. At most one per player and daily, and
  never once today's daily is done anywhere: the server checks again when a device picks it
  up.
- **Limits.** The same notification (`collapse_key`) comes once in a window, each kind has a
  daily cap, and all but the last call share `SOCIAL_PER_DAY` (8) (`notify::Rule`):

  | Kind | Same key once per | Per day |
  | --- | --- | --- |
  | `last_call` | daily | 1 |
  | `race` | group, 1 h | 3 |
  | `rival` | rival and daily | 3 |
  | `shared` | link, 3 h (then with the count of solves since) | 4 |
  | `group` | puzzle | 4 |

- **Blocks and bans.** Nothing comes from a player the recipient blocked (checked when queued
  and again when picked up) or from a banned or deleted player.
- **Data.** Outbox rows go two days after they were made, devices after 90 days without being
  seen. Deleting an account deletes its settings, devices and notifications, and the
  notifications about it (`notify::ERASE`); a guest's sign-in moves their devices (and their
  settings, if the account has none) to the account (`notify::MERGE_GUEST`). The privacy
  policy (`pages/legal.rs`, "Benachrichtigungen" / "Notifications") describes this and names
  the push services that get what, for every channel; a new channel or a change of what one
  sends has to go there too.

## Schema (migration 30, `server/db.rs`)

- `notification_prefs`: `user_id`, `enabled`, a column per kind (`last_call`, `race`,
  `rival`, `shared`, `group_puzzle`), `offer_shown`, `time_zone` (IANA, the last one a client
  reported), `updated_at`.
- `notification_devices`: `id`, `user_id`, `kind` (`web` | `apns` | `android`), `endpoint`
  (Web Push endpoint or APNs token, unique), `keys` (JSON: Web Push's `p256dh`/`auth`, APNs'
  `{"env": "production"}` or `"development"`, the push service the token is for),
  `secret_hash` (SHA-256 of the device's secret), `locale`, `time_zone`, `cursor` (the last
  outbox id the device got), `created_at`, `last_seen`. At most 10 per player.
- `notification_outbox`: `id` (AUTOINCREMENT, only grows), `user_id`, `kind`,
  `collapse_key`, `actor_id`, `args` (JSON), `path` (without the language), `created_at`,
  `expires_at` (a race's lobby, the daily's change).

## How it runs (`server/notify/mod.rs`)

- Social events queue at the moment they happen, in the background of the request that
  caused them (`notify::spawn`): `race_opened` (api/race.rs), `puzzle_posted` (api/kadai.rs),
  `rival_finished` (api/daily.rs, after a ranked submit), `shared_solved_code`
  (api/shares.rs). `enqueue` is one `INSERT … SELECT` that applies every rule, so two
  requests or two machines can't queue the same one twice.
- A job on every machine (`notify::start`, every 5 minutes, started in `main.rs`):
  `queue_last_calls`, `flush_shared` (solves held back by the 3-hour window, and those through
  races on shared puzzles), `deliver` through the configured channels, `delete_old`.
- A device gets rows with `id > cursor`. `pending` renders them and filters what no longer
  goes out (switched off, blocked since, last call for a daily done since, expired); `take`
  moves the cursor with a compare-and-set (`WHERE cursor = <old>`), which is what claims the
  rows: nothing is sent twice, a failed send is lost.

## Server functions (`api/notify.rs`)

| Function | Session | What |
| --- | --- | --- |
| `load_notification_prefs` | optional | the settings, `None` without a session |
| `save_notification_prefs(prefs, time_zone)` | required | saves (normalized) and notes the time zone |
| `answer_notification_offer(accept, time_zone)` | required | marks the offer shown, yes turns all on |
| `register_notification_device(kind, endpoint, keys, locale, time_zone)` | required | returns `{ id, secret }`; the same endpoint again replaces it |
| `unregister_notification_device(id, secret)` | none | forgets the device |
| `poll_notifications(device, secret)` | none | the device's new `PushMessage`s and when its next last call is due (`NotificationPoll`), `None` if unknown |

## Web Push (browsers)

Standard Web Push with our own key pair; no Firebase SDK or project, no web-push service. The
push service is the one the browser comes with: Google's for Chrome and most Chromium
browsers, Microsoft's for Edge, Mozilla's for Firefox, Apple's for Safari. It sees the
endpoint, the size, the timing and our signature, not the text.

- **Keys.** `VAPID_PUBLIC_KEY` (the uncompressed P-256 point, 65 bytes) and
  `VAPID_PRIVATE_KEY` (the scalar, 32 bytes), both base64url without padding, and optionally
  `VAPID_SUBJECT` (default `https://hashi.rs`; an `https:` or `mailto:` URL, we use no mail
  address). Fly secrets in production. `pnpm run vapid-keys` prints a new pair (Node's crypto,
  nothing leaves the machine). A new pair invalidates every browser's subscription, so keep it.
  Without the keys the channel is off and the pages don't offer it; half a pair or a pair that
  doesn't fit stops the server at startup (`validate_configuration`).
- **Server** (`server/notify/web_push.rs`, in `notify::channels()` when configured): the
  payload is the device's `PushMessage` as JSON (`title`, `body`, `url`, `tag`), encrypted per
  RFC 8291 (`aes128gcm`, one record, a fresh key and salt per message), signed per RFC 8292
  (ES256 JWT, `aud` the endpoint's origin, 12 hours). `TTL` until the outbox row expires (at
  most four weeks), `Urgency: high` for the last call and `normal` otherwise, `Topic` a hash of
  the tag so the push service keeps only the latest of one. 2xx is sent; 404 and 410 are
  `Delivery::Gone` (the device is forgotten), as is a subscription with an endpoint that isn't
  https or keys that don't parse; anything else is `Failed`. RustCrypto (`p256`, `aes-gcm`,
  `hkdf`, `sha2`, `base64ct`) and the `reqwest` the server already has.
- **The page** gets the public key in `<meta name="hashi-push-key">` (`app::shell`, empty when
  off). `state::web_push::here()` is `Ready` when the key is there and the browser has a
  service worker, `PushManager` and `Notification`; `HomeScreen` on an iPhone or iPad outside a
  home screen web app (Safari only pushes there, iOS 16.4 and later; the manifest's
  `display: standalone` makes the site one), where the card says how to add it; `No`
  elsewhere, and always in the apps (it is compiled into the website only).
- **Turning on** (the master switch, a kind with the master off, the offer's yes, the card's
  "Turn on here"): `Notification.requestPermission()`, then `/sw.js` is registered, then
  `pushManager.subscribe({ userVisibleOnly, applicationServerKey })` (a subscription for an
  earlier key is replaced), then `register_notification_device(Web, endpoint, keys, locale,
  time_zone)`. Id, secret, endpoint, locale and time go to local storage (`hashi.push.v1`).
  Only then are the settings saved; denied puts the switch back and shows how to allow them
  in the browser's site settings, a dismissed prompt just puts it back.
- **Turning off** (the master switch, "Turn off here", signing out, deleting the account):
  `unregister_notification_device(id, secret)` and `subscription.unsubscribe()`. The account
  card shows "This browser" with on or off whenever the account's switch or this browser is on.
- **Later visits** register again once a week or when the site's language changed
  (`state::web_push::refresh`, from `app.rs`), which keeps `last_seen` fresh; a subscription
  that is gone ends the registration.
- **The service worker** (`public/sw.js`, plain JavaScript with JSDoc types, checked by
  `tsc -p tsconfig.sw.json`) has a `push` handler (`showNotification` with the tag, the
  192 px icon and `badge-96.png`, a white silhouette of the favicon) and a `notificationclick`
  handler (focuses an open tab of the site and navigates it, else opens a window, only to
  the site's own URLs). No `fetch` handler and no caches, so nothing is ever served from it.
  It is served at its fixed URL with `Cache-Control: no-cache` like every unhashed file; the
  site has no Content-Security-Policy that would need a `worker-src`.

## Channels

Each one has its own `delivers(DeviceKind)` in `state/notify.rs` and its paragraph in the privacy
policy, which says what goes to whom.

1. **Web Push (website).** See [above](#web-push-browsers).
2. **APNs (iOS app).** See [The iOS app](#the-ios-app-apns).
3. **Android app.** No push service, see [The Android app](#the-android-app).

## The Android app

No push service: the app asks our server itself (`Notifications.kt` in
`app/src-tauri/gen/android/app/src/main/java/rs/hashi/`), with AndroidX's WorkManager, which needs
no Google Play services. The same code is in the Play and the foss (F-Droid) build.

- **Turning them on** (the master switch, a kind, or yes to the offer) asks Android for
  `POST_NOTIFICATIONS` first (13 and newer, `notify_permission` of the app's plugin). Refused,
  the switches go back and the page says to allow them in Android's settings. Allowed, the
  settings are saved, and `state::notify::follow` registers the phone
  (`register_notification_device(Android, None, None, locale, time_zone)`) and hands its id and
  secret, the server's URL and the `x-hashi-app` header to the plugin (`notify_start`), which
  keeps them in the app's private preferences, left out of Android's backups
  (`res/xml/backup_rules.xml`, `data_extraction_rules.xml`). The settings page does the same when
  it finds them on (turned on elsewhere) with Android's leave given, and stops when it finds them
  off.
- **Turning them off** (the master switch, or the last kind), signing out and deleting the
  account call `notify_stop`: the poll stops and a one-time job tells the server
  (`unregister_notification_device`) as soon as the phone is online.
- **The poll.** A periodic WorkManager job, every 15 minutes (its shortest period) with a
  network, calls `poll_notifications` the way the page calls server functions: `POST
  <server>/api/poll_notifications` with the JSON body `{"device": <id>, "secret": "<secret>"}`
  and the `x-hashi-app` header, no session. The answer is the JSON of `Option<NotificationPoll>`.
  `null` means the server forgot the phone (the account deleted, the device idle for 90 days):
  the app deletes its id and secret and stops. Each message is shown in the notification channel
  of its kind (`last_call`, `race`, `rival`, `shared`, `group`, and `other` for kinds a newer
  server has), named like the account page's switches in the phone's language
  (`res/values-*/notifications.xml`, written from the translations by the test
  `android_channels_are_named_like_the_switches` in `server/notify/render.rs`), so players can
  tune each in Android's settings too. The tag replaces an older one of the same tag, an expired
  message isn't shown and a shown one goes away when it expires (`setTimeoutAfter`). A tap opens
  the message's URL as an App Link of this app (the package named, so the debug build too), which
  the deep-link plugin hands to the page like any App Link (`api::native::follow_app_links`).
- **The last call's time.** Android stretches the 15 minutes to hours while the phone sleeps
  (Doze), too late for a reminder with a deadline. So every answer says when the player's next
  last call is due (`next_last_call`: today's while it can still come, else tomorrow's, `None`
  with the last call off), and the app sets an inexact alarm that may go off in Doze
  (`setAndAllowWhileIdle`, no exact alarm and no `SCHEDULE_EXACT_ALARM`) one minute after it.
  Such an alarm comes up to three quarters of its distance late (an hour at most), so it goes in
  steps: halfway there while the moment is more than 10 minutes away, then at the moment itself,
  at most about seven minutes late. At the moment the alarm polls (Android gives the app a few
  seconds and the network for that, also in Doze); a failed poll is retried by a one-time job
  once the phone is online. The server queues the player's due last call in the poll itself
  (`notify::poll`) instead of waiting for the next round of the 5-minute job, so the poll at the
  moment finds it, and checks again that today's daily isn't done.
- **Restarts.** WorkManager schedules its jobs again after a restart of the phone and an update
  of the app. The alarm is set again from the stored time by `LastCallReceiver` on
  `BOOT_COMPLETED` and `MY_PACKAGE_REPLACED`.
- **Test by hand** (the emulator test does the rest, see app/README.md): turn notifications on in
  the app, allow them, `adb shell dumpsys jobscheduler | grep rs.hashi` shows the periodic job;
  `adb shell cmd jobscheduler run -f rs.hashi <job id>` runs it now; a race opened by another
  member of a group shows up as a notification in the channel "Races", and a tap opens its lobby
  in the app. Turned off in Android's settings, the switch in the app goes back with the hint.

## The iOS app (APNs)

Straight from our server to Apple's push service (APNs) with our own key: no Firebase, no SDK and
no service in between. Unlike Web Push the message isn't encrypted for the device, only on its way
to Apple (TLS), so Apple sees the text; the privacy policy says so.

- **Key.** A token-based APNs auth key (`.p8`, "Hashi APNs", team scoped, Sandbox & Production)
  from Certificates, Identifiers & Profiles, Keys. `APNS_KEY_P8` (the file's text, a P-256 key in
  PKCS #8; newlines as `\n` work too), `APNS_KEY_ID` (its 10 characters), `APNS_TEAM_ID`
  (`Q35TCGM5PW`) and optionally `APNS_TOPIC` (default `rs.hashi`, the bundle id). Fly secrets in
  production. Without them the channel is off; part of them or a key that doesn't parse stops the
  server at startup (`validate_configuration`). The key doesn't expire; revoked in the portal, the
  sends fail with `InvalidProviderToken` until a new one is set.
- **Server** (`server/notify/apns.rs`, in `notify::channels()` when configured): one HTTP/2
  request per message (`reqwest` with HTTP/2 only, ALPN `h2` over rustls), `POST
  /3/device/<token>` to `api.push.apple.com`, or `api.sandbox.push.apple.com` for a device whose
  keys say `development`. `authorization: bearer <provider token>`, an ES256 JWT (`kid` the key
  id, `iss` the team, `iat`), one for every request until it is 40 minutes old (Apple takes it
  for an hour and refuses new ones more often than every 20 minutes). `apns-topic`,
  `apns-push-type: alert`, `apns-priority` 10 for the last call and 5 otherwise,
  `apns-expiration` when the outbox row expires (at most four weeks), `apns-collapse-id` the tag
  (a hash of it beyond 64 bytes). The payload: `aps.alert` with `title` and `body`,
  `aps.sound: default`, `aps.thread-id` the kind (the Notification Centre groups by it), and our
  `url` for the tap; more than 4 KB is `Delivery::Failed`. 200 is sent; 410 and 400
  `BadDeviceToken`/`DeviceTokenNotForTopic` are `Delivery::Gone` (the device is forgotten), as is
  a token that isn't hex or keys without the service; 403 with a refused provider token
  (`InvalidProviderToken`, `ExpiredProviderToken`) drops the token for the next request if it is
  at least 20 minutes old (a fresher one has a wrong key or id, which a new one wouldn't fix);
  that and anything else (429, 5xx) is `Failed`. The test against Apple's sandbox with the real
  key is `apple_s_sandbox_takes_our_provider_token` (ignored, run by hand: a made-up token must
  answer `BadDeviceToken`).
- **Entitlement.** `aps-environment` in `hashi_tauri_iOS.entitlements` is `development` as
  committed (the simulator, builds signed for development). The release workflow sets it to
  `production` before it builds (the App Store and TestFlight) and checks it in the IPA and its
  profile. Not a build setting (`$(…)`): Tauri signs the archive with the file as it is before
  Xcode exports it, and the export dropped the unexpanded value. The App ID `rs.hashi` has the Push Notifications
  capability; automatic signing makes the profiles with it.
- **The app's plugin** (`HashiPlugin.swift`): `notify_permission` asks iOS for alerts and sounds
  (`requestAuthorization`; iOS asks once, after a no only Settings, Notifications, Hashi changes
  it), `notify_state` says whether they are allowed, `notify_token` calls
  `registerForRemoteNotifications` and answers the token in hex and the environment (the
  `aps-environment` of the embedded provisioning profile, `production` without one as in the App
  Store and TestFlight, `development` in the simulator), or fails after 30 s. iOS gives the token
  to the application's delegate, tao's, which has no method for it: `main.mm` adds
  `application:didRegisterForRemoteNotificationsWithDeviceToken:` and `…didFailToRegister…` to
  its class before UIKit takes it, and they post the token to the plugin.
- **Turning on** (the master switch, a kind with the master off, yes to the offer): the
  permission first; refused, the switches go back and the page says to allow them in iOS's
  settings (`refused_ios`). Allowed, the settings are saved, then `state::notify::follow`
  registers the phone (`state::apns::register`: `notify_token`, then
  `register_notification_device(Apns, token, {"env": …}, locale, time_zone)`). Id, secret,
  token, environment, locale and time go to local storage (`hashi.apns.v1`).
- **Turning off** (the master switch, the last kind), signing out and deleting the account:
  `unregister_notification_device(id, secret)` from the page, and local storage forgets it.
- **Every start** (`state::notify::refresh` from `app.rs`): a registered phone asks for its
  token and registers again when iOS gave a new one (a restore, a reinstall), the language
  changed or a week went by (which keeps `last_seen` fresh); the earlier registration is
  unregistered. Without iOS's leave any more, the registration ends.
- **Showing and tapping** (`main.mm`): the notification centre's delegate, set before the app
  finishes launching, shows a notification that comes while the app is in front as a banner too
  (`willPresentNotification`). A tap hands the payload's `url` to the scene delegate's
  `scene:continueUserActivity:` as a web browsing activity, like a Universal Link, so the
  deep-link plugin passes it to the page (`api::native::follow_app_links`, only the app's own
  pages, `seo::app_link`). A tap that starts the app waits for its scene, and the response in the
  scene's connection options counts once.
- **Tests.** Unit: the provider token (verified with the key's public half), its renewal,
  the requests, the payload, the environment, the answers, and a fake APNs on localhost over
  HTTP/2 (h2c; the addresses are the channel's to set in the tests). The iOS smoke test
  (`e2e/ios/notifications.spec.ts`) allows them on the simulator, asks for a token (noted, the
  simulator of a runner may not get one), sends a notification with `xcrun simctl push` and taps
  its banner: the app shows its page. Not in CI: Apple's real service (the test above, by hand)
  and a real iPhone (TestFlight).
- **Test by hand** on an iPhone with a TestFlight build: turn notifications on in the app's
  account page, allow them; a race opened by another member of a group shows up within the
  5-minute job, and a tap opens its lobby in the app. Turned off in iOS's settings, the switch
  in the app stays on but the account card says to allow them in iOS's settings.
