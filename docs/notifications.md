# Notifications

Hashi tells players about five things, each with its own switch:

| Kind (`NotifyKind`) | When | Tap opens |
| --- | --- | --- |
| `last_call` | The player has a streak (yesterday's daily solved and ranked) and today's daily isn't done, about three hours before the daily changes, in their own time zone | `/` |
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
  its Web Push keys and the browser can take them (below), in the apps not until their channels
  land. Where it's `false` the offer never shows and the settings say that notifications can't
  reach this device yet. The switches still work: they are account settings.
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
  the push services that get what; it has to be extended when another channel lands.

## Schema (migration 30, `server/db.rs`)

- `notification_prefs`: `user_id`, `enabled`, a column per kind (`last_call`, `race`,
  `rival`, `shared`, `group_puzzle`), `offer_shown`, `time_zone` (IANA, the last one a client
  reported), `updated_at`.
- `notification_devices`: `id`, `user_id`, `kind` (`web` | `apns` | `android`), `endpoint`
  (Web Push endpoint or APNs token, unique), `keys` (Web Push `p256dh`/`auth`, JSON),
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
| `poll_notifications(device, secret)` | none | the device's new `PushMessage`s, `None` if unknown |

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

## What the next changes add

Each one adds a channel and flips its `delivers(DeviceKind)` in `state/notify.rs`, and
extends the privacy policy with what goes to whom.

1. **Web Push (website).** Done, see above.
2. **APNs (iOS app).** The p8 key, key id and team id as Fly secrets; the app's plugin asks for
   permission and hands the device token to the page, which registers `(Apns, token)`.
   Server: an `impl Channel` for `DeviceKind::Apns` over HTTP/2 with a JWT;
   `BadDeviceToken`/`Unregistered` answer `Delivery::Gone`.
3. **Android app.** A background worker (WorkManager) that calls `poll_notifications(id,
   secret)` every so often and shows what comes back, plus a local alarm around the last
   call's time (`timing::last_call_at`'s rule) that polls before it shows anything, so a daily
   finished elsewhere silences it. The app registers `(Android, None, None)` once and
   keeps id and secret in its storage. No server change beyond what is here.
