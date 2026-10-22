# Mobile apps

Plan for the Android app, then iOS with the same setup. Each step below is its own PR;
this file is updated as they land.

## Decisions

- **Tauri 2.** The app bundles a client-side rendered (CSR) build of the same Leptos UI
  and runs `backend` natively for offline practice. One UI, one language, no second
  code base to keep in sync.
- **Application ID `rs.hashi`** (Android `applicationId`, iOS bundle id). Fixed once the
  first build is uploaded to a store. Play only accepts IDs with at least one dot,
  so plain `hashi` is not possible. Users see the name "Hashi", not the ID.
- **Android first.** Every build runs in CI: a debug APK on pull requests, and a
  signed App Bundle uploaded to Google Play from a tag. iOS follows with the same
  workflow on a macOS runner and TestFlight.
- **Offline practice in the first release.** Generation, hints and checks of practice
  and created puzzles run on the phone. The daily, rankings, rivals and groups stay on
  the server, which keeps the daily's timing authoritative. The local solver is never
  used for today's daily.
- **The server is the API.** The app calls the existing server functions on
  `https://hashi.rs`. Their paths are already stable (`/api/<name>`, no hash, see
  `.cargo/config.toml`); from the first release on they are a contract that installed
  apps rely on for months.
- **Bearer sessions in the app, cookies on the web.** The app's WebView runs on its own
  origin, where the `SameSite=Lax` session cookie of hashi.rs is a third-party cookie
  that WebViews block. The app sends the same session token as
  `Authorization: Bearer hs1.<token>` and receives replacements in a response header.
  The web keeps its HttpOnly cookie.
- **Dojo uploads and admin stay on the website.** The app links to hashi.rs for them.

## Architecture

```
common · backend · sat_solver · hashi_i18n        shared crates
        │                        │
   hashi (web)                app/ (Tauri 2)
   Axum + Leptos SSR,         WebView: Leptos CSR build of the same UI
   hydrate in the browser     Rust: backend natively (offline practice)
        ▲                        │
        └──── HTTPS, /api/* ─────┘  bearer token, x-hashi-app version
```

The `hashi` crate gets a `csr` feature next to `hydrate` and `ssr`. Both `hydrate` and
`csr` enable `browser`, which marks code that needs a browser (audio, animation frames,
listeners). The app crate builds the CSR bundle with Trunk and wraps it with Tauri.

## Compatibility rules

Installed apps keep their version until the user updates. From the first release on:

- Never rename or remove a server function, never change its arguments incompatibly.
  Add a new function instead and keep the old one until no supported app calls it.
- Response enums that reach the client tolerate unknown variants.
- The app sends its version (`x-hashi-app`). The server answers versions below its
  configured minimum with `AppOutdated`, and the app asks for an update instead of a
  reload. Raise the minimum only after the store has offered the update for a while.
- CI checks the list of paths and the request and response JSON against golden files.

## API headers

What the app and the server send each other, on top of the server functions' JSON:

- `x-hashi-app: <platform>/<major>.<minor>.<patch>` on every call of the app, e.g.
  `android/1.0.0` or `ios/1.0.0`. It marks the request as the app's. Other platforms
  are accepted; a value of another form counts as an outdated app.
- `Authorization: Bearer hs1.<token>`: the app's session token, the same 64 hex digits
  as the web's cookie. The `hs1.` prefix keeps it apart from the admin and worker tokens.
- `x-hashi-session` in responses to the app: its new session token, empty when the
  session ended (logout, account deleted). Such responses are `Cache-Control: no-store`.
- `x-hashi-build` in every response: the build of the server (see `api::client`).
- `AppOutdated` (`E:app_outdated`): the answer to every `/api/*` call of an app below
  `HASHI_APP_MIN_VERSION`, encoded like any server function error, before the function
  or the rate limits run. The app asks for an update instead of a reload.
- CORS: the app's origins `https://tauri.localhost` (Android) and `tauri://localhost`
  (iOS), or `HASHI_APP_ORIGINS`; outside production also `http://localhost` and
  `http://127.0.0.1` on any port. GET and POST, the request headers `content-type`,
  `accept`, `authorization` and `x-hashi-app`; `x-hashi-build` and `x-hashi-session`
  are exposed; no credentials. Preflights are answered for two hours.

The app's side (`src/api/app.rs`, only in the `csr` build; the website sends none of it):

- Server: on the app's origins (`https://tauri.localhost`, `http://tauri.localhost`,
  `tauri://localhost`) the server functions go to `https://hashi.rs`, or to
  `HASHI_SERVER_URL` if it was set when the app was built. Anywhere else they stay on the
  page's origin, where a proxy passes `/api` on (`pnpm run app:web`, the smoke test).
- `x-hashi-app`: the platform from the user agent (`android`; `ios` for iPhone, iPad and a
  Mac with a touch screen; else `web`) and the version of `app/Cargo.toml`, which
  `hashi_app` passes to `hashi::mount`. Releases keep it equal to the store version.
- Session token: in local storage, `hashi.session`. Not HttpOnly like the cookie, but the
  `WebView`'s storage is private to the app. A response's `x-hashi-session` replaces it, an
  empty one removes it, unless the token changed while the call was out (an answer to a call
  sent before a sign in doesn't bring the old session back).
- Single flight: without a token, a call that may make a session (`start_daily`,
  `create_group`, `join_group`, `set_rival`, `login`, `register`) holds back every other
  call until its answer's token is kept, so calls at the same time don't make two guests.
- `AppOutdated`, and an answer that is a web page (a server function the server no longer
  has), show one toast with an "Update" button to the store, where the website offers a
  reload. Share and invite links point at `https://hashi.rs`; the Dojo's upload forms and the
  admin's house form are links to the website.

## Steps

| # | Step | Depends on | Status |
|---|------|------------|--------|
| 1 | This plan | | ✓ #108 |
| 2 | API contract: golden list of server function paths and request JSON; response enums tolerate unknown variants | | ✓ #111 |
| 3 | Bearer sessions: `Authorization: Bearer hs1.<token>` accepted next to the cookie, replacement tokens in `x-hashi-session` for requests that opt in | | ✓ #113 |
| 4 | `browser` and `csr` features, CSR entry point, CSR lint in CI | | ✓ #114 |
| 5 | App version header, minimum version and `AppOutdated`; CORS for the app origins | 2 | ✓ #119 |
| 6 | App client: configurable server URL, bearer token, version header, share links on hashi.rs, Dojo/admin links out | 3, 4, 5 | ✓ #PR |
| 7 | CSR shell (`app/`, Trunk, `index.html`, Tailwind) built in CI | 6 | |
| 8 | Tauri 2 Android project; CI builds a debug APK on every PR | 7 | |
| 9 | Offline practice: native generator and solver through Tauri commands | 8 | |
| 10 | Android polish: back button, safe areas, share/clipboard/links through Tauri plugins, App Links for `/g/<code>` invites | 8 | |
| 11 | Release workflow: tag `app-v<version>` builds a signed AAB and uploads it to the Play internal track | 8 | |
| 12 | First release: closed test, store listing, production | 9, 10, 11 | |
| 13 | iOS: same project, macOS runner, TestFlight and App Store | 12 | |

## Owner tasks

Things only the account owner can do, needed from step 11 on:

- Play Console: create the app `rs.hashi`, enable Play App Signing.
- A Google Cloud service account with release rights in the Play Console, its JSON key
  as GitHub secret.
- The upload keystore as GitHub secrets (generated once, a backup kept offline).
- Store listing: description, screenshots, privacy policy URL (`/datenschutz`), the
  Data safety form, content rating, and trader status (EU).
- Personal Play accounts created after 13 November 2023 need a closed test with at
  least 12 testers for 14 days before the first production release.
- iOS: Apple Developer Program membership and an App Store Connect API key.
