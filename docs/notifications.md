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
  browser or app has a way of delivery (`delivers(DeviceKind)`). It is `false` everywhere until
  the channels below land, so the offer never shows and the settings say that notifications
  can't reach this device yet. The switches still work: they are account settings.
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
  policy (`pages/legal.rs`, "Benachrichtigungen" / "Notifications") describes this and has to
  be extended when a channel lands.

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

## What the next changes add

Each one adds a channel and flips its `delivers(DeviceKind)` in `state/notify.rs`, and
extends the privacy policy with what goes to whom.

1. **Web Push (website).** VAPID key pair as Fly secrets; a service worker that shows the
   message and opens `url` on a tap; the page subscribes with `PushManager.subscribe` (only
   after the player turns something on or accepts the offer) and calls
   `register_notification_device(Web, endpoint, keys, …)`, again on later visits to keep
   `last_seen` fresh. Server: an `impl Channel` for `DeviceKind::Web` (RFC 8291 encryption,
   RFC 8292 VAPID) returned by `notify::channels()`; HTTP 404/410 answers `Delivery::Gone`.
   `delivers(Web)` becomes "the browser has `PushManager`".
2. **APNs (iOS app).** The p8 key, key id and team id as Fly secrets; the app's plugin asks for
   permission and hands the device token to the page, which registers `(Apns, token)`.
   Server: an `impl Channel` for `DeviceKind::Apns` over HTTP/2 with a JWT;
   `BadDeviceToken`/`Unregistered` answer `Delivery::Gone`.
3. **Android app.** A background worker (WorkManager) that calls `poll_notifications(id,
   secret)` every so often and shows what comes back, plus a local alarm around the last
   call's time (`timing::last_call_at`'s rule) that polls before it shows anything, so a daily
   finished elsewhere silences it. The app registers `(Android, None, None)` once and
   keeps id and secret in its storage. No server change beyond what is here.
