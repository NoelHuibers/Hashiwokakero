# Deployment

## Build and release

CI deploys the website and Dojo worker to Fly.io after their required checks pass
on `main`. [fly.toml](../fly.toml) and [dojo/fly.toml](../dojo/fly.toml) define the
apps. CI uses the prebuilt Dockerfiles; the standard Dockerfiles build from source
for manual deployments.

Both CI deployments share the [packaged compatibility gate](packaged-compatibility.md),
including site E2E/Docker checks and the packaged worker's real proof checkers. That
document describes its coverage limits and the additional branch-protection check.

Deploy the website and worker from the same revision. For separate manual
deployments, update the worker first.

For a deployment outside Fly, set `LEPTOS_ENV=PROD`, build with
`cargo leptos build --release`, and serve `target/site` with `target/release/hashi`.

## Configuration

Store credentials in the deployment platform's secret store.

- `HASHI_DAILY_SECRET` and `DOJO_SECRET`: independent random values of at least
  32 characters. Preserve existing production values across deployments;
  changing them changes future daily puzzles or seasonal benchmark sets.
- `DOJO_URL`: worker URL. Leave unset when solver evaluation is disabled.
- `DOJO_TOKEN`: at least 32 random characters, matching on the website and worker
  when evaluation is enabled.
- `HASHI_ADMINS`: comma-separated ids of existing registered accounts. Register
  the accounts before assigning their ids, and remove grants before deleting an
  admin account.
- `HASHI_ADMIN_TOKEN`: optional credential for administrative scripts. House-entry
  uploads from CI require the matching GitHub Actions secret.
- `TURSO_DATABASE_URL` and `TURSO_AUTH_TOKEN`: remote database connection. For a
  local database, set `HASHI_DB_PATH` to a persistent location; it defaults to
  `hashi.db`.
- `HASHI_APP_MIN_VERSION`: optional, the oldest app version (`x.y.z`) the server
  answers; older apps are asked to update (see [mobile.md](mobile.md#api-headers)).
  Raise it only when a server function changes incompatibly, and only after the
  stores have offered the update for a while: installed apps update late.
- `HASHI_APP_ORIGINS`: optional, comma-separated origins of the app's WebView that
  may call the API. Defaults to `https://tauri.localhost,tauri://localhost`.
- `HASHI_ANDROID_CERT_SHA256`: optional, comma-separated SHA-256 fingerprints
  (uppercase hex with colons, as keytool and the Play Console show them) of the
  certificates the Android app `rs.hashi` is signed with. The server lists them in
  `/.well-known/assetlinks.json`, which Android fetches to verify the app's App
  Links (invites and shared results open in the app, see
  [app/README.md](../app/README.md#app-links)); unset, that file is a 404 and the
  links open in the browser. Public, so it is in `fly.toml`: the upload key's
  certificate, which signs the APK of the GitHub release, and Google's app signing
  certificate (Play Console, Protected by Google Play, App signing, "App signing
  key certificate", SHA-256), which signs the installs from Play.
- `HASHI_IOS_APP_IDS`: optional, comma-separated app IDs of the iOS app, the team
  ID and the bundle ID (`Q35TCGM5PW.rs.hashi`, as the Apple developer portal shows
  them). The server lists them in `/.well-known/apple-app-site-association`, which
  Apple's CDN fetches for iOS to verify the app's Universal Links (the same links
  as Android's, see [app/README.md](../app/README.md#app-links)); unset, that file
  is a 404 and the links open in Safari. Public (every signed build of the app has
  it), so it is in `fly.toml`. After a deploy that changes it, check
  `curl -sI https://hashi.rs/.well-known/apple-app-site-association` (200,
  `application/json`) and Apple's copy,
  `https://app-site-association.cdn-apple.com/a/v1/hashi.rs`, which follows on
  Apple's own schedule.
- `HASHI_APP_STORE_ID` and `HASHI_PLAY_PACKAGE`: optional, the apps' pages in the
  stores, the iOS app's Apple ID (the digits of its `apps.apple.com/app/id…` link,
  App Store Connect, App Information: `6826916346`) and the Android app's package
  (`rs.hashi`). Public, in `fly.toml`, empty until the apps are live: set each once
  its app is publicly available in its store, not before (the site would link to a
  page that isn't there). With them, every page has Apple's Smart App Banner (iOS
  only, `<meta name="apple-itunes-app">`, which also opens the installed app on the
  same page), the footer the stores' official badges in the page's language
  (`public/badges/`, unchanged artwork from Apple's and Google's badge tools; Apple
  has none in Arabic and Hindi, Google none in Ukrainian, those get the English one),
  the home page the apps as structured data (`MobileApplication`, `sameAs` of the
  game), and `/manifest.webmanifest` the apps as `related_applications`
  (`prefer_related_applications: false`, the site stays installable on its own).
  Either one alone works too.

A malformed value of any of them stops the server at startup.

The worker also needs `APP_URL` pointing to the website. Its remaining settings
are documented in [dojo/fly.toml](../dojo/fly.toml).

## Maintenance

Back up production data before schema-changing deployments and verify that the
backup can be restored. For local SQLite, use a consistent database backup or stop
the server before copying it; copying the main file during WAL writes can omit data.

Increment `DOJO_SEASON` to start a new season and archive the previous one.
Keep `DOJO_SECRET` stable through this transition.

Changes to judging, fuel costs, or limits that affect scores require a
`dojo::RULES_VERSION` bump. Changes to generated benchmark sets also require a
`dojo::GENERATOR_VERSION` bump.

On Fly, client addresses come from its proxy. Direct deployments use the socket
peer; another reverse proxy requires an explicit trusted-proxy integration for
client-address handling.
