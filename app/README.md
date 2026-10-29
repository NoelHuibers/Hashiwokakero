# The mobile app

`hashi_app` renders the whole UI of the website in the browser, without a server rendering the
page first (client-side rendering, CSR). It is the web part of the mobile app; the plan and the
steps are in [docs/mobile.md](../docs/mobile.md). The Tauri project that wraps it for Android
and iOS is `app/src-tauri/`, a crate of its own outside the workspace, see [Android](#android)
and [iOS](#ios) below.

The crate is a WASM binary whose `main` calls `hashi::mount()`. It depends on `hashi` with the
`csr` feature only; `hashi` with `hydrate` or `ssr` (the website) never gets `csr` from it,
because Cargo only unifies the features of the packages it builds (`-p hashi` doesn't build
this crate).

## Files

- `index.html`: the static page, Trunk's input. It brings what `app::shell` in `src/app.rs`
  adds around `<App/>` on the website (see below).
- `Trunk.toml`: where the bundle goes, the dev server and its `/api` proxy, and two hooks: the
  Tailwind CLI from the pnpm packages (`style/tailwind.css`, run in the repository root like
  cargo-leptos runs it for the website) and a copy of `public/` to the root of the bundle,
  without the Dojo's development set (1.4 MB; the app's Dojo page links to the one on hashi.rs).
- `src/main.rs`: mounts the app, and fails the build if the server functions' paths have a
  hash (see below).

## Building

Trunk **0.22.0**, pinned in `Trunk.toml` (`trunk-version`) and in CI:

```sh
cargo binstall -y trunk@0.22.0
pnpm install          # Tailwind, and the fonts in public/fonts (or: pnpm run fonts)
```

From the repository root:

```sh
pnpm run app:build    # release bundle in target/app-dist
pnpm run app:web      # dev server on http://127.0.0.1:8080, rebuilds on changes
```

`app:web` passes `/api/...` on to a hashi server on `127.0.0.1:3000` (`pnpm run dev`), so the
server functions are same-origin calls and need no CORS. Its debug bundle goes to
`target/app-dev`.

The release build uses the website's `wasm-release` profile (`opt-level = "z"`, LTO, one codegen
unit, `panic = "abort"`) and `wasm-opt -Oz`. Trunk downloads the wasm-bindgen CLI of
`Cargo.lock` and wasm-opt itself.

It has to run inside this repository: `.cargo/config.toml` sets `DISABLE_SERVER_FN_HASH`, which
makes the server functions answer at `/api/<name>`. Built elsewhere, the paths get a hash the
server doesn't know. A `const` assertion in `src/main.rs` stops such a build.

Where the server functions go: see [Talking to the server](#talking-to-the-server).

## Talking to the server

The app calls the website's server functions as the website does, with what the API headers
section of [docs/mobile.md](../docs/mobile.md#api-headers) describes on top (`src/api/app.rs`):

- In the `WebView` (`https://tauri.localhost`, `tauri://localhost`) the calls go to
  `https://hashi.rs`. Another server is set when building, e.g.
  `HASHI_SERVER_URL=https://staging.example pnpm run app:build` (HTTPS: the `WebView`'s page
  is). The content security policy in `src-tauri/tauri.conf.json` only lets the app connect to
  `https://hashi.rs`: another server has to be added to its `connect-src` too. In a browser
  (`app:web`, the smoke test) the calls stay on the page's origin and its proxy.
- Every call sends `x-hashi-app: <platform>/<version>`, the version of this crate's
  `Cargo.toml` (`env!("CARGO_PKG_VERSION")` in `src/main.rs`). Release builds keep it equal to
  the version in the stores (docs/mobile.md, step 11): the server turns versions below
  `HASHI_APP_MIN_VERSION` away, and the app then asks for an update from the store.
- The session is a token in local storage (`hashi.session`), sent as
  `Authorization: Bearer hs1.<token>`, never a cookie.
- Share and invite links point at `https://hashi.rs`. Uploads to the Dojo and the admin's
  forms are links to the website, as are the Dojo's downloads.

An answer that is a web page asks for an update only when it is the server's 404 page (a server
function the server doesn't have any more). Other web pages, a proxy's 502 or 503 while the
server starts or is down, or a captive portal's, count as a call that didn't get through
(`api::client`, also on the website).

## On the phone

What the `WebView` doesn't do like a browser, and what the app does instead (`src/api/native.rs`
in the repository root, only in the app):

- **Links out of the app.** The `WebView` has no tabs: a link with `target="_blank"` or to
  another site would replace the app's page. A click listener sends every link to another origin
  and every `mailto:` link to Tauri's opener plugin, which hands it to Android (the browser, the
  Play Store for the update toast's link, the mail app); `window.open` of the share bar's
  WhatsApp and X buttons too. The capability allows `https://*` and `mailto:*` only: the pages
  link to GitHub, the SAT Competition, the GBD and the Dojo's sources (any https URL), so a list
  of hosts would break some of them. As a second line, the app refuses every navigation away
  from its pages (`on_navigation` in `src-tauri/src/lib.rs`, Tauri allows them all by default)
  and opens https ones in the browser.
- **Sharing.** Android's `WebView` has no `navigator.share`. The share buttons open Android's
  share sheet with the text through the app's own plugin (`HashiPlugin.kt`, command `share`).
- **Downloads.** The `WebView` ignores downloads of Blob links (the CNF of a puzzle, the
  workshop's puzzle as text). The file goes to the share sheet instead, written to the app's
  cache and lent to the receiving app by the `FileProvider` of the manifest: Files, Drive, a
  mail. Simpler than the dialog and fs plugins, and no storage permission.
- **The clipboard.** `navigator.clipboard.writeText` works in the `WebView`
  (`https://tauri.localhost` is a secure context, the copy is a tap), no plugin needed.
- **Edge to edge.** The page draws behind the status and the navigation bar (Android 15 enforces
  it for apps targeting it). The `WebView` reports no `env(safe-area-inset-*)` for the bars, so
  `MainActivity.kt` passes their sizes in as CSS variables, `--app-inset-top`, `-right`,
  `-bottom` and `-left`, and `style/tailwind.css` makes `--safe-*` of them, which the header
  (top), the tab bar, the toasts and the page's end (bottom), and the body (sides) use. On the
  website they are unset (the bottom keeps following `env()`), so it looks as before. The bars'
  icons follow the page's theme, not the phone's: the theme's effect calls the plugin's
  `system_bars`.
- **The network.** With `ACCESS_NETWORK_STATE` the `WebView` knows when it is offline:
  `navigator.onLine` and the window's `online` event, which brings the offline notice back.
- **The soft keyboard.** Edge to edge, the window no longer shrinks for the keyboard
  (`adjustResize` only hands its size to the app now), so it covered the inputs at the bottom of
  the page. `MainActivity.kt` ends the `WebView` above it instead: the page's viewport shrinks as
  in a browser, the focused input scrolls into view, the tab bar sits on the keyboard, and the
  page's bottom inset is 0 while the keyboard covers the navigation bar.
- **App Links.** Links to the app's pages on `https://hashi.rs` open in the app, see
  [App Links](#app-links).
- **Games services.** The stamps are Google Play Games achievements (Game Center's on iOS), see
  [Games services](#games-services).

On iOS the `WebView` is WebKit's `WKWebView`, which does more of it itself. The app's own plugin
has its iOS side in `src-tauri/ios/` (`HashiPlugin.swift`, see [iOS](#ios)). Where it differs:

- **Safe areas.** With `viewport-fit=cover` the page draws under the status bar, the Dynamic
  Island and the home indicator, and the sides held sideways, and `WKWebView` reports them as
  `env(safe-area-inset-*)`, in portrait and landscape, on the iPad too. `index.html` sets
  `--app-inset-*` to them, no native code. On Android the variables `MainActivity.kt` sets on
  the element win over that rule. With `viewport-fit=cover` WebKit sets the page's scroll view's
  `contentInsetAdjustmentBehavior` to `never` itself, so nothing is inset twice, and wry turns
  the scroll view's bounce off: no band of another colour above or below the page. Landscape
  needs the scene lifecycle (`UIApplicationSceneManifest` in `Info.plist`, tao's
  `TaoSceneDelegate`): without it UIKit kept the portrait insets when the phone was turned (top
  62, sides 0 on an iPhone 17 under iOS 26), and the page went under the Dynamic Island.
- **Sharing and downloads.** `WKWebView` has `navigator.share` (`tauri://localhost` is a secure
  context, so `navigator.clipboard` works too), but the page takes the same path as on Android,
  the plugin's `share`: `UIActivityViewController` with the text, or with the file
  (written to the app's temporary folder), which the sheet offers to save in Files. Blob
  downloads don't work in the `WebView` either. On the iPad the sheet is a popover in the
  middle of the page.
- **The status bar** follows the page's theme through the window's style
  (`overrideUserInterfaceStyle`), which the share sheet and the keyboard take on too.
- **The soft keyboard.** Nothing to do: `WKWebView` scrolls the focused input above the
  keyboard, the page's visual viewport ends at it, and the tab bar sits on it, also while the
  page scrolls. Unlike Android the bottom inset stays (the home indicator's 34 px under the tab
  bar's labels) while the keyboard is open.
- **Links out** go to Safari and Mail through the opener plugin's iOS side, the same capability
  and `on_navigation` guard as on Android (the page's origin is `tauri://localhost`).
- **Back.** iOS has no back button: the swipe from the screen's left edge goes back in the
  page's history, the router's pages, like Android's back gesture
  (`allowsBackForwardNavigationGestures`, which wry sets on macOS only). The pages are one
  document, so it never leaves the app; at the first page it does nothing.
- **App Links** are Universal Links on iOS, see [App Links](#app-links).

The smoke test checks the session (a guest made in the app, no cookie, the same guest after a
reload), the headers, one update toast however many calls the server refuses, and the links.

## App Links

Where the app is installed, the phone opens links to some pages of `https://hashi.rs` in the app
instead of the browser (App Links on Android, Universal Links on iOS), the ones people send each
other: an invite to a group (`/g/<code>`), the daily (`/`, where a daily result's share text
points), practice (`/practice`, the practice's), the groups (`/groups`, `/groups/...`) and
players' pages (`/player/<id>`), each in every language (`/ja/g/<code>`). Not the rules, the legal
pages, the admin's, the Dojo with its uploads, or anything the server answers itself (`/api`,
`/og`, the downloads). `seo::APP_LINK_PATHS` and `APP_LINK_PREFIXES` list them;
`plugins.deep-link.mobile` in `src-tauri/tauri.conf.json` for Android, `seo::app_link_components`
for iOS. Tests in `seo.rs` compare the three (and print the JSON to paste after a change, a new
language say) and check that Apple's patterns match the same paths as Android's.

- **The app:** `tauri-plugin-deep-link` (official) writes, when the app is built, an intent
  filter with `android:autoVerify="true"` for these paths into the manifest (between its
  `DEEP LINK PLUGIN` markers, committed like the rest of `gen/android`), and `applinks:hashi.rs`
  into the Associated Domains of the iOS app's entitlements (`gen/apple`, committed; the iOS job
  fails if a build changes them). iOS takes the paths from the site, not from the app. The page
  asks the plugin for the link the app started with (`get_current`, allowed in the capabilities)
  and listens for the next ones (Tauri's event `deep-link://new-url`), the same on both platforms,
  and its router shows their page (`api::native::follow_app_links`), keeping the language of the
  link: the one the app started with instead of the start page, later ones on top of the page the
  app was on, which the back button goes back to. Other URLs change nothing. The link never becomes
  a navigation of the `WebView`, so the guard that sends other sites to the browser
  (`on_navigation`) doesn't see it. Links to these pages from the app's own pages: there are none
  (share and invite links are copied or shared, not followed); one would go to the system, which
  brings it back to the app.
- **iOS** hands a Universal Link to the scene delegate: tao's `TaoSceneDelegate`
  (`scene:continueUserActivity:`) turns the link of a running app into Tauri's `RunEvent::Opened`,
  where the plugin keeps it for `get_current` and sends the event. The app runs on the scene
  lifecycle (for the safe areas in landscape, see [On the phone](#on-the-phone)), where a cold
  start's link comes in `scene:willConnectToSession:options:`, which tao 0.37 doesn't pass on:
  `gen/apple/Sources/hashi_tauri/main.mm` passes it on to `scene:continueUserActivity:` after tao's
  own method (see [iOS](#ios)). Universal Links
  open the app when a link is tapped in another app (Messages, Mail, Safari from another site);
  typed into Safari's address bar, or tapped on a page of hashi.rs itself, they stay in Safari.
- **The server** says which app may open its links. For Android `/.well-known/assetlinks.json`
  names `rs.hashi` and the SHA-256 fingerprints of its signing certificates, from
  `HASHI_ANDROID_CERT_SHA256` (docs/deployment.md; a 404 without it). Android fetches it when the
  app is installed and opens the links in the app only if the installed app's certificate is
  listed. The upload key's is in `fly.toml` (it signs the APK of the GitHub release); the app from
  Play is signed by Google, whose certificate the owner adds once the app is in Play
  (docs/mobile.md, owner tasks). Debug builds (`rs.hashi.debug`) are never verified: the links
  open them only when an intent names the package, as the emulator test's do.
- For iOS `/.well-known/apple-app-site-association` (no extension, `application/json`) names the
  app ID `Q35TCGM5PW.rs.hashi` (team ID and bundle ID, from `HASHI_IOS_APP_IDS` in `fly.toml`; a
  404 without it) and the paths as `components`: `{"/": "/practice"}` for a page, `{"/": "/g/*"}`
  for the pages below a prefix. The exact file is `src/golden/files/apple-app-site-association.json`.
  iOS doesn't fetch it from the site but from Apple's CDN
  (`https://app-site-association.cdn-apple.com/a/v1/hashi.rs`), which refreshes it now and then:
  after a change of the paths, phones can take a while to see it (installs and updates fetch it). A build
  only gets the links when it is signed with the team (`bundle.iOS.developmentTeam`) and its
  provisioning profile has Associated Domains, so the unsigned simulator build in CI never opens
  them (`xcrun simctl openurl` sends them to Safari). On a device with a TestFlight build:
  Settings, Developer, Universal Links (with the developer mode on) diagnoses a URL, and
  `swcutil` on a Mac shows what the device fetched.

## Games services

The stamps (`Stamp::ALL`) are achievements of the phone's games service too, and a few results go
to its leaderboards: Game Center on iOS, Google Play Games (Play Games Services v2) on Android.
The server stays the source of truth: the page reports what the server says the player has, the
platforms only show it. Both keep the most a player ever reached (an achievement never goes back,
a leaderboard keeps the best score), so the same report again changes nothing.

The page (`src/api/native.rs`, the arguments in `src/api/games.rs`, nothing on the website):

- `games_sign_in()`, once at the start (`mount`), silently: the platform signs in whoever is
  signed in on the phone. Its answer, `{ available, signedIn }`, is shared by every later call,
  until a sign-in through the button (`games_show`, below).
  `available`: the phone has the service, always on iOS, on Android only in a build with Play
  Games' project id (below).
- `games_report(achievements, scores)`: each achievement as `{ id, steps, target, earned }`
  (`steps` of `target` for a stamp that counts, `None` for one that is only earned), each score as
  `{ id, value }`, with the platform's ids (Apple's vendor ids; Play's own, from
  `app/achievements-play.json`). `games::Report::new` sends only what there is to say: the steps
  within `1..=target`, all of them once earned, an unearned stamp without steps not at all. It
  runs in the background; the plugin answers once the platform took it (iOS) or queued it
  (Android) and skips it while nobody is signed in.
- When the page reports (`native::games_sync`, `games_progress`): at the start, once the sign-in
  signed a player in, and after each daily the server took (the win screen's `submitted`, or a
  finish the recovery found), or stamps the server announces (yesterday's medal, a Dojo stamp),
  then from the server's statistics (`my_stats`, the stamp book's call), and on the account page
  from its own. `games::report` makes the report: every stamp of the book, earned or its
  progress as steps of `Stamp::steps`, and the scores `puzzles_solved`, `best_streak` and, on
  Game Center only, `daily_time` of today's daily solved ranked (none of 0). The ids are Apple's
  vendor ids on iOS; on Android Play's, from `app/achievements-play.json` as it was when the page
  was built (`include_str!`): a stamp or leaderboard missing there isn't reported. The page sends
  only what changed since its last report (`games::Reported`), the next start everything again.
  A report the plugin refuses (offline, a platform's error) is logged and forgotten, so the next
  report sends it again. Without a signed-in player nothing is asked or sent, until a sign-in
  through the button.
- `games_show(Board::Achievements | Leaderboards)`: the service's own screens. The stamp book of
  the account page has a button for the achievements ("Show in Game Center",
  `stamps.games.show` with the service's name, which isn't translated), in the app where the
  service is available. The plugin answers once it showed the screens, signing the player in
  first if nobody was. A player the start didn't sign in is then asked about again
  (`games::shown`): signed in now, every later call of `games_sign_in` gets that answer and
  `games_sync` runs once, so the stamps go to the platform without a restart.

All of it is a no-op outside the app, and a failure is only logged, never shown. The commands are
`games_sign_in`, `games_report` and `games_show` of the app's plugin (`build.rs`,
`capabilities/default.json`).

**iOS** (`HashiPlugin.swift`, GameKit, which Swift links by itself for `import GameKit`, as for
Tauri's own plugins; the entitlement `com.apple.developer.game-center` is in
`gen/apple/hashi_tauri_iOS/hashi_tauri_iOS.entitlements`, which the build leaves as it is):

- The sign-in sets `GKLocalPlayer.local.authenticateHandler`. When nobody is signed in, GameKit
  hands over its sign-in sheet: the plugin shows it by itself once per install
  (`hashi.gameCenterPrompted` in `UserDefaults`), never in the simulator (no Apple Account there,
  and the smoke test starts every test as a fresh install), and otherwise keeps it for the
  button. It answers the page once GameKit called the handler, or after 10 s (GameKit doesn't
  call it again when the sheet is cancelled).
- A report loads the player's achievements first and sends only those that got further:
  `percentComplete` of the steps, 100 once earned. Game Center's banner shows when exactly one
  achievement is completed by it (a stamp just earned), not for a row of them at once (the stamps
  from before, at the first sign-in), where the page's own toasts already speak. Scores go
  through `GKLeaderboard.submitScore`, one board each. The command answers once GameKit answered
  all of them, refused if one failed.
- The screens open through `GKAccessPoint.shared.trigger(state:)`, with the access point's
  floating button off: it would sit over the page's header. `GKGameCenterViewController` ends
  with iOS 26 (deprecated in its SDK), and the access point presents and dismisses the screens
  itself. Without a signed-in player the button shows the sign-in sheet GameKit kept, if it
  handed one over; the command answers once GameKit calls the handler again with the player
  signed in, never if the sheet is cancelled.
- The release checks that the signed app has the entitlement and its App Store profile Game
  Center (`app-release.yml`).

**Android** (`HashiPlugin.kt`, `com.google.android.gms:play-services-games-v2`):

- Play Games' project id, the numeric one of the Play Console, is a build input
  (`gen/android/app/build.gradle.kts`): release builds take `project_id` of
  `app/achievements-play.json`, next to Play's ids of the achievements, any build
  `HASHI_PLAY_GAMES_APP_ID` of the environment instead. It isn't secret. It goes into the manifest's `com.google.android.gms.games.APP_ID` (as a string
  resource) and `BuildConfig.PLAY_GAMES_APP_ID`.
- **Empty, `null` or no file means no Play Games**: the SDK's provider (`PlayGamesInitProvider`, which
  starts it with the app since version 22) is off in the manifest (`playGamesEnabled`), and the
  commands never touch the SDK: `games_sign_in` answers `available: false`, `games_report` does
  nothing, `games_show` refuses. Debug builds have no id unless the environment sets one: they
  are `rs.hashi.debug`, which Play Games only signs in with a credential of its own. The
  emulator test checks that the app's process has no Play Games provider.
- With an id the SDK signs in by itself at the start (automatic sign-in); `games_sign_in` asks
  for its result (`isAuthenticated`). A report sets a counting achievement's steps (`setSteps`;
  Play unlocks it at its total, which has to be the stamp's target, and the achievement has to be
  incremental in the Play Console) and unlocks the others (`unlock`), and submits the scores
  (`submitScore`); Play sends them when it can. The button signs in first if nobody is (`signIn`,
  the one time Play Games may ask) and opens `achievementsIntent` or `allLeaderboardsIntent`; the
  command answers when the player leaves that screen (so a new player's stamps go then).
- The release checks the provider against the id and names it in the job summary.

To turn Play Games on: the Play Games Services project in the Play Console (its credentials for
`rs.hashi` with the app signing key's and the upload key's SHA-1, the testers), then its project
id into `project_id` of `app/achievements-play.json` in a pull request, whose release dry run
checks the bundle. Game Center needs its capability on the App ID: Apple Developer,
[Certificates, Identifiers & Profiles](https://developer.apple.com/account/resources/identifiers/list),
Identifiers, `rs.hashi`, Capabilities, Game Center ticked, Save. Without it the release's export
fails with "Automatic signing cannot update bundle identifier": the API key's automatic signing
can't add it. To try Play Games with a debug build: a
credential for `rs.hashi.debug` with the debug key's SHA-1, then
`HASHI_PLAY_GAMES_APP_ID=<id> cargo tauri android build --debug --apk --target aarch64`.

## Offline

Practice works without a network: in the app, generating a puzzle, hints, checks, revealing the
solution, the workshop's uniqueness check and the CNF of a puzzle run on the phone. The
website's server functions `generate_puzzle`, `solve_puzzle` and `puzzle_cnf`
(`src/api/puzzle.rs`) are also Tauri commands of the same names (`src-tauri/src/lib.rs`), which
call the same `backend` functions with the same limits as the server: a solver run between
100 ms and 20 s (`backend::api::player_config`), two computations at a time and six waiting
(`src-tauri/offline`). A seed gives the phone the puzzle the server generates for it (the
generator counts work, not time); a test in `src-tauri/offline` keeps the versions of everything
the generator uses (rand, splr) equal in both `Cargo.lock`s. After Dependabot updates one of
them in the repository's lock, update the app's: `cargo update -p <name> --precise <version>`
in `src-tauri`.

- `src/api/practice.rs` decides: in the app, where Tauri's `window.__TAURI_INTERNALS__` is there,
  the commands, else (the website, the bundle in a browser) the server functions. The pages call
  `api::practice` instead of the server functions. Only the app's build (`csr`) has the bridge,
  `src/api/native.rs`, which calls Tauri's `invoke` without Tauri's npm package.
- A command takes the server function's JSON request body as a string (`{"args": "..."}`) and
  answers with the JSON of its answer, also a string: JavaScript's numbers would round the
  64-bit seeds. Errors are the server function's texts (`E:wrong_puzzle`, ...).
- The commands are app permissions (`allow-generate-puzzle`, ... generated by `build.rs`), which
  `capabilities/default.json` grants to the main window only.
- A computation can't be stopped once it runs; the page drops answers it no longer needs (the
  epochs of `src/state/solver.rs`), and every computation ends within its budget.

**Today's daily.** The server refuses to solve today's daily for players who haven't finished it.
The phone checks the daily it saved (`hashi.daily.v1`): if the puzzle is that daily, however it
is turned, the daily is today's and not won or revealed, the solvers refuse with the server's
error (`daily_locked`). It isn't enforced: a player who didn't open today's daily on this phone
gets it solved, like anyone who runs another solver (on the website too). The lock on the open
puzzles of a player's groups (`puzzle_locked`) isn't repeated on the phone, which doesn't know
which of them are still open.

**Without a network** the app starts and practice works. The pages that need the server show their
own state (the daily: "Couldn't load today's puzzle"); a call that can't reach the server shows
one notice, "You're offline. Practice still works.", instead of an error toast each
(`state/toasts.rs`), again only after the window's `online` event. The website shows its errors
as before.

## index.html and app::shell

On the website the server renders `app::shell`, whose `<head>` the static page has to repeat:

- the meta tags (charset, the viewport with `viewport-fit=cover`, author, color-scheme),
- the icons and the web manifest,
- `THEME_SCRIPT`, inline before the first paint, so night mode doesn't flash. The app's content
  security policy allows it by its hash: `app_page_has_the_theme_script` in `src/app.rs` fails
  when the copy here differs from the constant, or when the page gets another inline script.
  Trunk doesn't minify the page (`minify = "never"`), which would rewrite it,
- `/fonts/fonts.css` and the Tailwind stylesheet, which the website links through
  `HashedStylesheet`.

Not needed here: `MetaTags` (leptos_meta adds the per-page tags to the `<head>` in the browser
itself), `HydrationScripts` (Trunk adds the loader), `AutoReload`, and the `hashi-build` meta
(the app sends its version instead).

When `app::shell` changes, change `index.html` with it.

## Tests

`e2e/app/smoke.spec.ts`, with its own Playwright config, apart from the website's tests: the
bundle starts, the practice board shows a generated puzzle, the router opens the rules, a
language prefix like `/ja/rules` works, also after a reload, and the saved theme applies. It
serves the bundle like the app's WebView does (every unknown path is `index.html`) and passes
`/api` on to a server it starts on a fresh database:

```sh
cargo leptos build --release
pnpm run app:build
pnpm run app:e2e      # E2E_BIN, E2E_SITE_ROOT, E2E_PORT, E2E_APP_DIST as for pnpm run e2e
```

`e2e/app/offline.spec.ts` plays the app without a network: a fake `window.__TAURI_INTERNALS__`
records the commands and answers them, every `/api` request fails. Practice generates, hints and
checks through the commands, the workshop's uniqueness check refuses today's unfinished daily,
the daily shows its offline state, and the ranks one offline notice.

CI runs both in "Smoke test app (web)" on the bundle of "Build app (web)", whose artifact
`app-web` the Android build uses. Neither is a required check.

## Android

`src-tauri/` is a Tauri 2 project (crate `hashi_tauri`, Tauri 2.12, tauri-cli 2.12.4) that shows
the bundle in the system WebView. It has its own `Cargo.lock` and is excluded from the workspace
(`exclude` in the root `Cargo.toml`), so the website's builds never compile Tauri. It still builds
inside the repository, where `.cargo/config.toml` applies. Its workspace has a second crate,
`offline/` (`hashi_offline`), the offline practice without Tauri (see [Offline](#offline)), whose
tests run on any machine. It uses `backend` without its command line (`default-features = false`,
no clap).

- `src/lib.rs`: `run`, the app's entry point on the phones (`tauri::mobile_entry_point`), and
  `src/main.rs`, the same on the desktop, for development only.
- `tauri.conf.json`: application ID `rs.hashi`, the name, the version, the bundle
  (`frontendDist`), the one window, and the content security policy (below).
- `capabilities/default.json`: what the page may call in Tauri: `core:default`, the offline
  practice's commands, the opener's `open_url` for `https:` and `mailto:` URLs, the deep-link
  plugin's `get_current`, and the app's own plugin `hashi` (`share`, `system_bars`, `games_*` of
  [Games services](#games-services)). `build.rs` lists the commands of the app and of its plugin,
  which makes Tauri check them against the capabilities.
- Plugins: `tauri-plugin-opener` (official, links out of the app, its own click script off),
  `tauri-plugin-deep-link` (official, [App Links](#app-links)), and `hashi`, the app's own, in `src/lib.rs` with its Android side in `gen/android`
  (`HashiPlugin.kt`) and its iOS side in `ios/` (`HashiPlugin.swift`, see [iOS](#ios)). See
  [On the phone](#on-the-phone).
- `icons/`: generated from `public/icon-512.png` by `cargo tauri icon app-icon.json`, along with
  the Android launcher icons in `gen/android/.../res/mipmap-*` and the iOS ones (see [iOS](#ios)). Android's adaptive icon cuts a
  circle (or a squircle) out of the middle of its foreground, so the foreground,
  `icons/android-foreground.png`, is the badge alone, small on a transparent square; the
  background is the website's `#f4ecdd`. Made with ImageMagick, in the repository root:

  ```sh
  magick public/icon-512.png -fill none -draw "color 0,0 floodfill" \
    -gravity center -background none -extent 756x756 app/src-tauri/icons/android-foreground.png
  ```

- `gen/android/`: the Android project (Gradle), generated by `cargo tauri android init` and
  committed with its Gradle wrapper. Changed by hand: the window's colours (the website's
  background, light and dark, so the screen doesn't flash white before the page paints; the
  `WebView` itself is transparent until then), the icons, the checksum of the Gradle download,
  the manifest (`ACCESS_NETWORK_STATE`, `adjustResize`; no Android TV launcher; the App Links'
  intent filter is the deep-link plugin's; Play Games' project id and provider, see
  [Games services](#games-services)), `build.gradle.kts` (signing, the NDK, Play Games'
  project id), `MainActivity.kt` (the system bars and the keyboard, see [On the phone](#on-the-phone)) and `HashiPlugin.kt`, and the template's
  unused `activity_main.xml` removed. Build outputs and the files Tauri writes on every build
  are in its `.gitignore`s.

### Prerequisites

A JDK 17 or newer (with `javac`), the Android SDK and NDK, and the Rust target:

```sh
sdkmanager "platform-tools" "platforms;android-37.0" "build-tools;36.0.0" "ndk;29.0.14206865"
export ANDROID_HOME=~/Android/Sdk NDK_HOME=$ANDROID_HOME/ndk/29.0.14206865 JAVA_HOME=...
rustup target add aarch64-linux-android      # x86_64-linux-android for the emulator
cargo binstall -y tauri-cli@2.12.4
```

Android Studio brings all of that except the Rust parts; without it, the
[command line tools](https://developer.android.com/studio#command-line-tools-only) have
`sdkmanager`. Gradle downloads what is still missing on the first build. NDK 29 links the
library for Android's 16 KB memory pages, which Google Play requires. Its version is in three
places, changed together: `NDK_VERSION` in `ci.yml` (both Android jobs) and in
`app-release.yml`, and `ndkVersion` in `gen/android/app/build.gradle.kts` (Gradle strips
release builds with it). The web bundle needs what [Building](#building) lists.

### Building and running

From `app/src-tauri/`:

```sh
cargo tauri android build --debug --apk --target aarch64
adb install -r gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk
cargo tauri android dev     # with the page from the dev server
```

`build` runs `pnpm run app:build` first (`beforeBuildCommand`), then builds the library with
the bundle inside and packs the APK with Gradle. Debug builds are signed with the local debug
key and have the ID `rs.hashi.debug`, so they install next to the store's app. `dev` starts
`pnpm run app:web` and loads the page from it (`devUrl`, Trunk's dev server); for a phone on
Wi-Fi, pass `--host` and let Trunk listen on that address (`trunk serve --address`).

The architectures: `aarch64` (arm64-v8a) is nearly every phone of the last years and the one
pull requests build. The store's App Bundle ([Releases](#releases)) adds `armv7` (older and cheap
32-bit phones) and `x86_64` (Chromebooks, emulators): Play delivers only the device's own, so more
of them only make the upload bigger. `i686` isn't worth it.

### In CI

"Build app (Android)" in `.github/workflows/ci.yml` takes the bundle of "Build app (web)" from
its artifact `app-web` and skips `beforeBuildCommand`
(`--config '{"build":{"beforeBuildCommand":null}}'`), so the WASM is built once. It runs Clippy
on both crates for Android, Clippy and the tests of `offline/` on the runner (Tauri itself
would need WebKitGTK there), then builds the debug APK for arm64 without debug info and uploads it
as the artifact `android-debug-apk` (kept one day for pull requests, seven for main), with its
size in the job summary. JDK 17 from `actions/setup-java`, the SDK of the runner image with the
NDK pinned, tauri-cli pinned. Not a required check. Dependabot updates the crate's dependencies
and the Gradle project; after a Tauri update, compare `gen/android` with a fresh
`cargo tauri android init` in a scratch copy, the template changes with Tauri.

### The WebView's origin

The page runs at `https://tauri.localhost` (`useHttpsScheme: true` in `tauri.conf.json`).
**This can never change after the first release**: localStorage (saved games, settings, the
session token) belongs to the origin, and an app on another origin starts empty. The server's
CORS list (step 5) has to allow it.

### Content security policy

`app.security.csp` in `tauri.conf.json`:

```
default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline';
img-src 'self'; font-src 'self'; connect-src 'self' https://hashi.rs ipc: https://ipc.localhost;
object-src 'none'; base-uri 'self'
```

- `script-src`: Tauri adds the SHA-256 hashes of the page's two inline scripts (the theme
  script and Trunk's loader) and of the bundle's `.js` file when it builds the app, and sends
  the policy as a header with `index.html`. `'wasm-unsafe-eval'` lets the page start the WASM.
- `style-src 'unsafe-inline'`: the UI sets `style` attributes (Leptos `style=`), which need it.
  It only works without hashes or nonces next to it, so Tauri must not add any
  (`dangerousDisableAssetCspModification: ["style-src"]`).
- `connect-src`: the bundle itself, the server, and Tauri's IPC (`https://ipc.localhost` on
  Android with the https origin, `ipc:` on iOS and the desktop). A development server on another
  host needs `devCsp`.

### Version and versionCode

`version` in `tauri.conf.json` is the version users see (`versionName`). Tauri derives Android's
`versionCode`, which must grow with every upload to Play, from it:
`major × 1,000,000 + minor × 1,000 + patch`, so 0.0.1 is 1, 0.1.0 is 1000 and 1.2.3 is 1,002,003
(minor and patch stay below 1000). `scripts/app-version.mjs` sets it, see [Releases](#releases).

## Emulator test

`e2e/android/smoke.spec.ts`, with its own Playwright config (`e2e/android.config.ts`), starts the
real debug APK on an Android emulator and drives its WebView through Playwright's Android support
(`_android`, the WebView's debugging socket, which Tauri opens in debug builds). Each test clears
the app's data and starts it with `am start`: it hydrates and knows it is offline
(`navigator.onLine`), no page of the app breaks the content security policy
(`securitypolicyviolation` events and the console's CSP messages on `/`, `/rules`, `/ja/rules`,
`/practice`, `/create`, `/lab`, `/dojo`, `/groups`, `/daily`), practice generates, hints and
checks through the native commands, the ranks show one offline notice (again after an `online`
event), Android's back button goes back from the practice to the rules instead of closing
the app, and without Play Games' project id the games commands say the service isn't there and
its SDK never starts.

What only the phone shows: in the light and the dark theme, the header starts below the status
bar and the tab bar's labels end above the navigation bar (their boxes against the bars'
frames of `dumpsys window`), the page shows behind the bars and their icons stand out (the
darkest and brightest pixels of the bars in `screencap`); in the phone's dark mode no frame of
the start is bright; a link to GitHub and a `location` to another site leave the app's page
where it is and start a `VIEW` intent (the logcat); the share bar's copy works, its share button
and the CNF button open Android's share sheet (a `CHOOSER` intent), the CNF as a file in the
app's cache. Screenshots of the practice board, `practice-light.png` and `practice-dark.png`,
are taken with `screencap`.

The debug APK calls the production server when it can. The whole test runs without a network,
so it never makes a guest or starts a daily there: the global setup turns airplane mode, Wi-Fi
and mobile data off and waits until nothing answers before it installs the APK, the app never
starts online, and a test fails if any answer comes from hashi.rs. CI also blocks hashi.rs on the
runner.

CI runs it in "Android emulator smoke" (`.github/workflows/ci.yml`), next to the arm64 build: it
builds an x86_64 debug APK from the `app-web` artifact, boots a Google APIs x86_64 emulator of
API 35 with KVM (`reactivecircus/android-emulator-runner`, a cold boot of about 35 s: a cached
snapshot was 2 GB and took longer to restore), and runs the spec. About 5 minutes in all. Emulators are flaky, so this config, and
only this one, retries a failed test once; the job summary counts the flaky tests next to the
times of the build, the emulator's start and the tests. The artifact
`android-emulator-screenshot` has the practice board, light and dark; a failed run also uploads
`android-emulator-report`, the Playwright report and the app's, WebView's and Tauri's lines of
the logcat. Not a required check.

With an emulator or a phone on adb (airplane mode goes on, and stays on afterwards):

```sh
cargo tauri android build --debug --apk --target x86_64   # in src-tauri/, aarch64 for a phone
pnpm run app:android-e2e     # E2E_APK=<path> for another APK
```

## Releases

A release is a tag `app-v<x.y.z>` on main. `.github/workflows/app-release.yml` builds both
platforms from it:

- Android: the App Bundle for Google Play, signed with the upload key and uploaded to Play's
  internal test track; from there it is promoted in the Play Console (closed test, production).
- iOS: the IPA for the App Store, signed by Xcode with the App Store Connect API key and uploaded
  to App Store Connect, where it shows up in TestFlight; from there it goes to the testers, and
  later to App Review (see [iOS releases](#ios-releases)).

### Cutting one

1. A pull request with the new version and its release notes:

   ```sh
   node scripts/app-version.mjs 0.2.0     # every file below; --code prints the versionCode
   $EDITOR app/store/whatsnew/whatsnew-en-US
   ```

   The script sets the version in `app/Cargo.toml` (what the app sends in `x-hashi-app`),
   `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json` (the store's version and versionCode), the
   two crates' entries in `Cargo.lock` and `src-tauri/Cargo.lock`, and the iOS project's
   `CFBundleShortVersionString` and `CFBundleVersion` in `gen/apple` (see [iOS](#ios)). It refuses
   a version whose minor or patch is 1000 or more, or whose versionCode isn't above the current
   one. "Test & lint" fails when the files disagree (`node scripts/app-version.mjs` without a
   version).
   `whatsnew-en-US` is Play's "What's new" (at most 500 characters); other languages go next to
   it as `whatsnew-<locale>`, e.g. `whatsnew-de-DE`.
2. After the merge, tag main's merge commit and push the tag:

   ```sh
   git fetch origin
   git tag -a app-v0.2.0 -m "Hashi 0.2.0" origin/main
   git push origin app-v0.2.0
   ```

   The tag can follow the merge right away: the workflow waits for main's CI run of the commit.

The version is the same on both platforms: `versionName` and `CFBundleShortVersionString`. The
versionCode is the build number of both, Android's `versionCode` and iOS's `CFBundleVersion`.

### What the workflow checks

Before it builds: the tag is `app-v<x.y.z>` and equals the version in all the files, the tagged
commit is on main, and CI's run on main for that commit passed (it waits up to an hour for one
still going; a cancelled or failed one fails the release, re-run it or tag a newer commit).

After it builds `cargo tauri android build --aab --target aarch64 armv7 x86_64` (release): the
bundle and the APK are signed with the upload key (their certificate is the key's, not a debug
key's), `bundletool validate` passes, the ID is `rs.hashi` (not `.debug`), versionCode and
versionName are the version's, the bundle isn't debuggable, it has the native library and its
symbols for all three architectures, and the 64-bit libraries are aligned for 16 KB pages (the
ELF segments, and `zipalign -P 16` of the APK). The job summary has the sizes (the bundle, the
download for an arm64 phone as `bundletool get-size` computes it) and the SHA-256 fingerprint of
the upload key.

### Where it goes

- The tag's GitHub release (a pre-release): `hashi-<version>.aab` and
  `hashi-<version>-universal.apk`, every architecture in one APK for testers who install it by
  hand. Both are signed with the upload key, which Play only uses to check that an upload is
  ours; Play signs the app it delivers with its own app signing key. So they are safe to publish,
  and a hand-installed APK and the app from Play can't update each other.
- Google Play, the internal track, with the release notes of `app/store/whatsnew/`. The native
  symbols (`debugSymbolLevel = "SYMBOL_TABLE"`) and R8's mapping are in the bundle, Play takes
  them from there. Skipped, with a notice in the job summary, while the `PLAY_SERVICE_ACCOUNT_JSON`
  secret isn't set. While the app itself is still a draft in the Play Console, Play only accepts
  draft releases: set the repository variable `PLAY_RELEASE_STATUS` to `draft` until then (the
  default is `completed`).
- The workflow's artifact `android-release`, the same two files (30 days).
- For iOS: App Store Connect, `hashi-<version>.ipa` on the GitHub release, and the artifact
  `ios-release` (see [iOS releases](#ios-releases)).

Run by hand (`gh workflow run app-release.yml --ref <ref>`), or by a pull request, the workflow
is a dry run: it builds, signs and checks, keeps the files as the artifacts for three days, and
publishes nothing. On a branch it skips the checks of the tag, of main and of CI. A pull request
builds the platforms whose files it changes: both for the workflow or `scripts/app-version.mjs`,
Android for `gen/android/app/build.gradle.kts`, iOS for `gen/apple/` or `src-tauri/Info.ios.plist`.
Without the signing secrets (pull requests of Dependabot or of forks) an Android dry run signs
with a throwaway key, and iOS is skipped with a notice. With `-f publish=true` on a tag
(`--ref app-v0.2.0`) it is the full release again, e.g. to upload a version once the service
account exists. The GitHub release then gets its files replaced, and the IPA's notes when the
IPA is new.

### The first release

Play's API can only upload to an app that already has a bundle: the first one has to be
uploaded by hand. Create the app `rs.hashi` in the Play Console, push the first tag (the upload to
Play is skipped or fails), download `hashi-<version>.aab` from its GitHub release and upload it to
the internal test track by hand, accepting Play App Signing (Google keeps the app signing key, the
upload key only signs uploads). From then on, with the service account set up
([docs/mobile.md](../docs/mobile.md#owner-tasks)), tags upload by themselves.

### iOS releases

"Build IPA (iOS)" runs on `macos-26` with Xcode 26.6, as ["In CI"](#in-ci-1) does, and builds
with the secrets `APPLE_API_KEY_ID`, `APPLE_API_ISSUER` and `APPLE_API_KEY_P8`, an App Store
Connect team key with the Admin role (Admin is needed to make certificates):

```sh
cargo tauri ios build --ci --target aarch64 --export-method app-store-connect \
  --config '{"build":{"beforeBuildCommand":null},"bundle":{"iOS":{"bundleVersion":"<versionCode>"}}}'
```

The key's `.p8` goes into a file in the runner's temp dir, which is deleted at the end of the
job, also when the job fails. Tauri gets it through `APPLE_API_KEY`, `APPLE_API_ISSUER` and
`APPLE_API_KEY_PATH`, and the team through `APPLE_DEVELOPMENT_TEAM` (`Q35TCGM5PW`, in the
workflow; the same as `bundle.iOS.developmentTeam`, which it overrides). It archives the app unsigned. Xcode then exports the archive with automatic signing
(`-allowProvisioningUpdates` with `-authenticationKeyPath`, `-authenticationKeyID` and
`-authenticationKeyIssuerID`): it makes or reuses the team's cloud-managed Apple Distribution
certificate and the App Store profile of `rs.hashi`. Its private key stays with Apple, and no
certificate or profile is kept in the repository's secrets.

- **The build number.** `CFBundleVersion` is the versionCode (`node scripts/app-version.mjs
  --code`, 2 for 0.0.2), passed as `bundle.iOS.bundleVersion`. App Store Connect needs a higher
  one for every upload of a version, and it refuses four numbers, which `--build-number` would
  give (`0.0.2.1`). `CFBundleShortVersionString` is the version. In `gen/apple` both stay the
  version (`scripts/app-version.mjs`): only the release's build changes them. Xcode's export
  must not pick another one: with the key it asks App Store Connect and takes the next free
  number once a version's build is there (3 for 0.0.2 in pull requests after its upload), so
  `gen/apple/ExportOptions.plist`, which Tauri's export starts from, turns
  `manageAppVersionAndBuildNumber` off.
- **Export compliance.** `src-tauri/Info.ios.plist` sets `ITSAppUsesNonExemptEncryption` to
  false (the app only uses HTTPS, which is exempt), so TestFlight doesn't hold every build until
  the encryption question is answered. Every `tauri ios build` merges that file into
  `gen/apple`'s `Info.plist`, which has the key committed too, as `project.yml` does.
- **The checks.** The signature verifies. The certificate is an Apple Distribution one of team
  `Q35TCGM5PW`, and the profile is an App Store one (no devices, `beta-reports-active` for
  TestFlight, `get-task-allow` false). `application-identifier` is `Q35TCGM5PW.rs.hashi`, the
  Associated Domains have `applinks:hashi.rs`, and `CFBundleShortVersionString`,
  `CFBundleVersion` and `ITSAppUsesNonExemptEncryption` are as above. The binary is arm64. The job
  summary has the certificate, the profile, the SDK and the sizes.
- **The upload.** Publishing exports the same archive again, straight to App Store Connect:
  `xcodebuild -exportArchive` with `destination: upload` in its export options (symbols
  included, `manageAppVersionAndBuildNumber` off) and the same key. This is Xcode's own upload,
  the one the Organizer uses, and it fails the step when App Store Connect refuses the build.
  Apple processes the build (minutes to an hour, with an email) before it appears under
  TestFlight in App Store Connect. Internal testers then get it in the TestFlight app.
- **The IPA** (`hashi-<version>.ipa`, on the GitHub release and in the artifact `ios-release`)
  is the same archive exported for App Store Connect. It is signed for App Store distribution,
  so it only installs through TestFlight or the App Store. A failed or skipped iOS build doesn't
  hold up Android's release files, and a re-run adds the IPA.
- **Without the secrets** iOS is skipped with a notice in the job summary, and Android goes on.

**Re-running an upload.** If the upload failed, "Re-run failed jobs" on the run, or
`gh workflow run app-release.yml --ref app-v<version> -f publish=true`, builds and uploads again.
That only works while App Store Connect doesn't have the build: it refuses a build number it
already has, e.g. after a run whose upload went through but whose GitHub release failed. To ship
a new build then, bump the version (`scripts/app-version.mjs`) and tag again, as for Play.

### Signing

The upload key is a PKCS12 keystore with the key `upload`, in the repository's secrets
`ANDROID_KEYSTORE_BASE64` (the file, base64), `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and
`ANDROID_KEY_PASSWORD`. The workflow writes it to a file in the runner's temp dir and deletes it at
the end of the job, also when the job fails. `gen/android/app/build.gradle.kts` signs release
builds with it when it finds the key: in `gen/android/keystore.properties` (gitignored; `storeFile`,
relative to `gen/android/` or absolute, `storePassword`, `keyAlias`, `keyPassword`), else in the
environment (`ANDROID_KEYSTORE_FILE`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`,
`ANDROID_KEY_PASSWORD`, what the workflow uses). Without either, release builds stay unsigned;
debug builds always use the local debug key. To try a signed release build locally, make a
throwaway key, never a copy of the real one:

```sh
keytool -genkeypair -keystore /tmp/test.p12 -storetype PKCS12 -alias upload -keyalg RSA \
  -validity 10000 -dname CN=test -storepass testtest
ANDROID_KEYSTORE_FILE=/tmp/test.p12 ANDROID_KEYSTORE_PASSWORD=testtest ANDROID_KEY_ALIAS=upload \
  ANDROID_KEY_PASSWORD=testtest cargo tauri android build --aab --target aarch64
```

## Store listings

What the stores show of the app, its name, texts and images in every language, lives in the
repository, and `.github/workflows/store-listing.yml` uploads it with
[fastlane](https://docs.fastlane.tools) (pinned in `fastlane/Gemfile.lock`): `supply` for Google
Play, `deliver` for the App Store (`fastlane/Fastfile`). Only the listing: the workflow uploads no
build and never touches a release, a track, the review submission or its information, the age
rating, the privacy labels or Data safety form, the price or the release settings. Those are set by
hand in the Play Console and App Store Connect.

### Texts

In fastlane's layout, one folder per store locale:

- `fastlane/metadata/android/<locale>/`: Google Play's `title.txt` (at most 30 characters),
  `short_description.txt` (80) and `full_description.txt` (4000).
- `fastlane/metadata/<locale>/`: the App Store's `name.txt` and `subtitle.txt` (30 each),
  `keywords.txt` (100 bytes), `promotional_text.txt` (170), `description.txt` (4000) and the
  URLs: `support_url.txt` (`/kontakt`), `marketing_url.txt` (the home page) and `privacy_url.txt`
  (`/datenschutz`), each the site in the listing's language. `copyright.txt` is for every locale.

`store/locales.mjs` lists the locales of each store and the site language their screenshots are
taken in: all 20 of the site's languages, Spanish and Portuguese twice (es-ES and es-419/es-MX,
pt-PT and pt-BR, each written for its region). A locale with `same` gets a copy of another's texts
when the upload tree is put together, so they are written once: on Play fr-CA (fr-FR), es-US
(es-419) and zh-HK (zh-TW); on the App Store en-GB, en-AU, en-CA (en-US) and fr-CA (fr-FR).
Locales without a listing of their own fall back to the default language, en-US.

The texts are written for each language, not translated word for word, with the site's own words
for things (locales/: "die Brücke von heute", "Stempelheft", 鬼, ...). For the stores' search:

- **The name** (Play's title, the App Store's name) weighs most. "Hashi" first, then what people
  search for in that language: "Daily Bridges Puzzle", "Tägliches Brückenrätsel", "橋をかけろ".
- **The App Store** indexes the name, the subtitle and the keywords, not the description. The
  keywords are single words or short phrases, comma separated without spaces; every word of the
  name or the subtitle counts already (and plurals of them), so it doesn't go into the keywords
  again, nor "app", "game" or the category. **Google Play** indexes the title, the short and the
  full description: the short description has the main terms (Hashiwokakero, logic puzzle,
  bridges), the description uses them naturally.
- **Never**: names of other games, puzzle brands or apps (no "Sudoku", "Nikoli", "Kakuro"), the
  other store's platform, emoji or capitals for emphasis in names, rankings or price claims ("#1",
  "best", "free" in a name; "free, no ads" in a description is fine), calls to action ("download
  now").

`node store/validate.mjs` (`pnpm run store:validate`) checks all of it: every locale there and
known to fastlane, the limits (keywords in UTF-8 bytes, the safe reading for Chinese, Japanese and
Korean), the keyword rules, the forbidden words and the URLs, and the screenshots' captions.
`scripts/tests/store-listing.test.mjs` tests the checks; "Test & lint" runs it with the other
script tests.

The release notes ("What's new") are not part of this: the release workflow uploads them with each
build (`app/store/whatsnew`, [Releases](#releases)).

### Images

Made in CI, not committed (a run makes about 600). The sources:

- `store/capture/`: a Playwright run (`store.capture.ts`) on the built site with a seeded local
  database (`seed.ts`, made-up players, never Turso), the same server the browser tests start
  (`e2e/server.ts`). Every language of the site, six scenes (`scenes.ts`): today's daily in
  progress, the leaderboard with mika on the podium, a hard practice board, the group of friends,
  the stamp book and the solved daily at night. Each on a phone (402 × 874, three times) and a
  tablet (1032 × 1376, twice), with room left for the system bars as the app leaves it
  (`--app-inset-*`). The dailies are fixed puzzles (`puzzles.json`), one per difficulty: the page
  names the weekday's, so a run on a Monday shows the easy 7×7, on a Sunday the 24×24 Oni.
- `store/templates/frame.html`: a caption (`store/captions.json`, per language, `*word*` in
  vermilion) and a hanko over the device with the screenshot in it, in the site's colours, fonts
  and waves. `store/capture/devices.ts` has the sizes: the App Store's iPhone 6.9" (1320 × 2868)
  and iPad 13" (2064 × 2752), Google Play's phone (1080 × 1920) and 7" and 10" tablets
  (1080 × 1920, 1440 × 2560), all 9:16 or Apple's sizes, opaque PNGs.
- `store/templates/feature.html`: Play's feature graphic (1024 × 500), like the site's social
  preview. Play's hi-res icon is `public/icon-512.png`, the app's icon.

```sh
cargo leptos build --release              # the site, as for the browser tests
pnpm run store:images                     # target/store/images/<language>/<frame>/<n>-<scene>.png
STORE_LANGS=en,de,ja pnpm run store:images    # a few languages
STORE_FRAME_ONLY=1 pnpm run store:images      # only frame again (after changing a template)
pnpm run store:assemble                   # the upload tree, see below
```

`node store/assemble.mjs` puts together `target/store/fastlane/` for fastlane: the texts with the
copies of `same`, and the images sorted into supply's and deliver's folders. It checks every
image's size and that none has an alpha channel (Play's icon has one). With `--text` it leaves the
images out.

### Uploading

`.github/workflows/store-listing.yml`:

- **A pull request** that changes the listings, `store/` or the workflow: the checks, then all the
  images, kept for a week as the artifact `store-listing-images` to look at. It uploads nothing.
- **By hand** (Actions, Store listing, Run workflow; or `gh workflow run store-listing.yml -f
  platform=both -f what=all`): `platform` play, apple or both; `what` text, images or all (text
  needs no images and is quick). `validate_only` lets Google Play check the listing without saving
  it (Play's API validates an edit); App Store Connect has no such check, its upload is skipped
  then. Play: supply needs a release on a track to attach the listing to; it reads the internal
  track's, whose only release is the newest upload (`play_version_code` picks one when it has
  several). The App Store: deliver edits the version in "Prepare for Submission" (no new version,
  no build, never submitted), replaces its screenshots, and precheck then reports Apple's metadata
  rules as warnings in the log.

The keys are the release workflow's secrets, `PLAY_SERVICE_ACCOUNT_JSON` and
`APPLE_API_KEY_ID`/`APPLE_API_ISSUER`/`APPLE_API_KEY_P8`, written to files in the runner's temp
dir (fastlane's API key JSON for Apple) and deleted at the end of the job, also when it fails. To
run a lane locally (from the repository root, after `node store/assemble.mjs`):

```sh
BUNDLE_GEMFILE=fastlane/Gemfile bundle install
PLAY_JSON_KEY=/path/to/key.json BUNDLE_GEMFILE=fastlane/Gemfile bundle exec fastlane android listing what:text validate_only:true
APPLE_API_KEY_JSON=/path/to/key.json BUNDLE_GEMFILE=fastlane/Gemfile bundle exec fastlane ios listing what:text
```

## Game achievements

The 48 stamps of the stamp book are achievements on Apple's Game Center and on Google Play Games
Services, next to three leaderboards. The server decides what a player has; the app reports it to
the platform (the `games_*` commands of the `hashi` plugin). Both sides take the ids, points and
steps from `src/model/achievements.rs`:

- `Stamp::achievement_id`: `stamp_<id>` (`stamp_first`, `stamp_solved_100`). Game Center's vendor
  identifier; Play makes up its own IDs (`CgkI…`), which `app/achievements-play.json` maps these
  to. An achievement missing there isn't reported on Android, so the app works before Play has
  them.
- `Stamp::points`: multiples of 5 from 5 to 50, 855 of Game Center's 1000 together (Play allows
  2000), the rest kept for stamps to come. Can't be changed once an achievement is live.
- `Stamp::steps`: the stamp book's target for stamps that count up to one (10 dailies, 30 days in a
  row, 1000 bridges...): incremental on Play, a percentage on Game Center. `None` for those
  earned at once and for the full moon, whose target is the month's length.
- Hidden on both: the book's secret stamps. Game Center wants a description before it's earned
  even so: the book's neutral "A secret stamp. Keep playing to find it!".
- `GamesLeaderboard`: `daily_time` (today's daily, lower is better), `puzzles_solved` and
  `best_streak` (all time). `daily_time` is only on Game Center, as a leaderboard that starts over
  every day at midnight UTC with the puzzle day. Play's daily leaderboards start over at 07:00 UTC
  all year (its "midnight Pacific Daylight Time"), which can't be changed: one of its days would
  mix two puzzles, and its all-time view would rank Monday's easy times above every other day's.
  Game Center has no format for milliseconds: the time goes there in hundredths
  (`GamesLeaderboard::apple_score`).

A test in `src/model/achievements.rs` writes them to `store/achievements/model.json` for the
upload tooling and fails when that is out of date (`UPDATE_ACHIEVEMENTS=1 cargo test
--no-default-features --features ssr -p hashi --lib achievements` writes it).

### Catalogue and icons

`node store/achievements/catalogue.mjs` (`pnpm run store:achievements`) puts together what each
platform gets, in the store locales of `store/locales.mjs` (Game Center 26, Play 25): the names
and texts of `locales/<language>/stamps.json`, the leaderboards' names of
`store/achievements/leaderboards.json`. It checks the limits: Play's names ≤ 100 and descriptions
≤ 500 characters, each name once per locale, steps ≤ 10000, points; Game Center's ids, ≤ 100
points each and 1000 together. Apple documents no lengths for its texts: they are held to 100
(names) and 255 characters (descriptions). It writes `target/store/achievements/catalogue.json`.

The icons are the stamps' hanko, pressed like the stamp book does, with the tier's number where
stamps share a kanji: `store/templates/hanko.html`, taken by Playwright like the store images
(`store/capture/achievements.icons.ts`), but without the site: only the fonts `pnpm install`
writes to `public/fonts`. Game Center's at 1024 × 1024, Play's at 512 × 512, opaque, nothing
outside the circle Android masks Play's icons with. Then `catalogue.mjs` checks them and writes
Play's import ZIP (below).

```sh
pnpm run store:icons          # target/store/achievements/icons/{apple,play}/<id>.png
pnpm run store:achievements   # the checks, catalogue.json, play-import.zip
```

### Uploading

`.github/workflows/achievements.yml` ("Game achievements"):

- **A pull request** that changes the catalogue, the texts of the stamps, the icons' template or
  the workflow: the checks and their tests (`scripts/tests/achievements.test.mjs`), both uploads
  against an empty App Store Connect and Play project in memory (`store/achievements/fake.mjs`;
  `--offline`), and all icons with Play's import ZIP, kept as the artifact `achievements`.
- **By hand** (Actions, Game achievements, Run workflow; or `gh workflow run achievements.yml -f
  platform=both`): `platform` apple, play or both. `validate_only` reads what the platform has and
  lists what an upload would change, without changing anything.

Each upload looks at what is there first and only changes what differs, so a second run changes
nothing and never makes a second achievement. Nothing is ever deleted, submitted or published.

- **Game Center** (`store/achievements/apple.mjs`, the App Store Connect API; fastlane has no
  action for it): switches Game Center on for the app and for the iOS version being prepared,
  then every achievement and leaderboard by its vendor id. Since the API's version 4.2 their
  texts and images hang on versions: the run edits the latest one while it is being prepared,
  and makes a new one when it is live (it goes to review with the next app version). An image is
  uploaded per locale, named by its content's hash, so a changed icon is replaced. The first run
  sends about 4000 requests, more than App Store Connect's 3600 an hour: it waits when told to,
  up to the job's three hours; a run stopped halfway is just run again. The daily leaderboard's
  first day is the next midnight UTC, set when it is created.
- **Play Games** (`store/achievements/play.mjs`, the Games Services Publishing API with the
  service account of the releases): finds each achievement and leaderboard by its ID in
  `app/achievements-play.json`, else by its English name, else creates it; sets type, steps,
  hidden, points, names and descriptions in every locale. The new IDs are in the job summary and
  the artifact `achievements-play`: that file replaces `app/achievements-play.json` in a pull
  request (a workflow can't push to main, and a pull request it opened wouldn't run CI). What
  Play's API can't do: icons (its image upload is gone), the order, publishing. Type, steps and
  hidden can't change once published.

Locally, with the keys in the environment (`APPLE_API_KEY_ID`, `APPLE_API_ISSUER`,
`APPLE_API_KEY_P8`; `PLAY_SERVICE_ACCOUNT_JSON`, the key file or its contents):

```sh
node store/achievements/apple.mjs --offline   # against an empty App Store Connect, no key
node store/achievements/apple.mjs --dry-run   # what an upload would change
node store/achievements/play.mjs --offline
node store/achievements/play.mjs --dry-run --ids /tmp/ids.json
```

### In the consoles

Once, before the first upload, in the **Play Console** (in German; Hashi, then):

1. Mehr Nutzer gewinnen (or Wachstum), Play-Spieldienste, Einrichtung und Verwaltung,
   Konfiguration: "Neues Play-Spieldienste-Projekt erstellen", the Cloud project
   `hashi-play-512817` (the service account's; only its owners see it in the list), Verwenden.
   Under the game's name is now its Projekt-ID (12 digits): into `project_id` of
   `app/achievements-play.json` by a pull request (the Android app needs it too). Done
   2026-10-28: `909299474789`.
2. On the same page, Eigenschaften bearbeiten: the display name "Hashi", the default language
   English (United States) (the upload finds achievements by their en-US names), and the
   translations: every Play language of `store/locales.mjs`. Speichern.
3. Anmeldedaten, Anmeldedaten hinzufügen, Android: first the OAuth consent screen (Konfigurieren,
   in Google Cloud: External, then In Produktion on its Zielgruppe page, which wants the home page
   and the privacy policy's URL under Branding; Play Games' scopes need no verification), then
   OAuth-Client erstellen with the package `rs.hashi` and the SHA-1 of the
   app signing key (the page the SHA-256 for `fly.toml` came from, docs/mobile.md: App-Signatur,
   the app signing key's certificate); a second credential with the upload key's SHA-1 for
   builds not installed from Play. Änderungen speichern.
4. The service account may edit the game's services: Nutzer und Berechtigungen,
   `play-release@hashi-play-512817.iam.gserviceaccount.com`, Kontoberechtigungen: "Google
   Play-Spieldienste-Projekte bearbeiten". In the Google Cloud console, project
   `hashi-play-512817`, APIs und Dienste, Bibliothek: enable "Google Play Game Services
   Publishing API".
5. The achievements with their icons: the artifact `achievements` of a run (any pull request's),
   `play-import.zip`. Play-Spieldienste, Einrichtung und Verwaltung, Erfolge, Erfolge
   importieren, Hochladen, the ZIP, Als Entwurf speichern. Only into a project without
   achievements: the names are unique. (Without the import the upload creates them, with no icon:
   each is then set by hand on its page.)
6. Run the workflow (`-f platform=play`, first with `-f validate_only=true`): it adds the
   descriptions and translations and the leaderboards, and the IDs come out for
   `app/achievements-play.json`. The leaderboards' icons by hand: Bestenlisten, each one, Symbol,
   `icons/play/<id>.png` of the artifact.
7. Tester: yourself and the testing tracks, so the drafts work in test builds. Publishing
   (Veröffentlichung, Spiel veröffentlichen) once the app version reporting them is out, at least
   two hours before its rollout.

In **App Store Connect** nothing is needed before the first run, which switches Game Center on;
create the iOS version that will report them (1.1) first, so it is switched on there too. The app
needs its entitlement and the Game Center capability on the App ID, which the release's
automatic signing can't add ([Games services](#games-services)). Afterwards:

1. Check the leaderboard "Today's daily" (Apps, Hashi, Services, Game Center, Leaderboards): a
   recurring one, every day for a day, from a midnight UTC. Apple documents the recurrence's
   format only loosely; if it refused it, the run failed at `daily_time`: create it there by hand
   (Recurring, ID `daily_time`, Elapsed Time to the hundredth, Low to High, Best Score, start at
   00:00 UTC, duration and restart 1 day), then run the workflow again, which finds it by its id
   and adds its texts and images.
2. The first achievements and leaderboards go to review with an app version: on the version's
   page, Game Center, choose them all, then submit the version as usual. Later ones can go on
   their own.

## iOS

The same crate in an Xcode project, `gen/apple/`, made by `cargo tauri ios init` (xcodegen, from
its `project.yml`; tauri-cli 2.12.4 as for Android). Everything Tauri does for iOS needs a Mac with
Xcode: the project is generated, built and run there or in CI on a macOS runner, never on Linux.
This repository's project was generated in CI and committed like `gen/android`, with Tauri's
`.gitignore` for the build outputs (`build/`, `Externals/`). Changed by hand, or by the build:

- The icons in `Assets.xcassets/AppIcon.appiconset`: `cargo tauri icon app-icon.json` writes them
  there once `gen/apple` exists (instead of `icons/ios/`), on the website's background `#f4ecdd`,
  but with an alpha channel, which the App Store refuses for the 1024 px icon. After running it, in
  `src-tauri/`:

  ```sh
  for f in gen/apple/Assets.xcassets/AppIcon.appiconset/*.png; do
    magick "$f" -background '#f4ecdd' -alpha remove -alpha off -define png:color-type=2 "$f"
  done
  ```

- What the build writes into the project, committed so that a build leaves it as it is: the
  deep-link plugin puts `applinks:hashi.rs` (the App Links' host) into the Associated Domains of
  `hashi_tauri_iOS.entitlements` (Universal Links, step 13d of docs/mobile.md), and Tauri rewrites
  bits of `project.pbxproj` and `Info.plist`.
- The version. Every build writes the version of `tauri.conf.json` into `Info.plist`, as
  `CFBundleShortVersionString` and, through `agvtool`, as `CFBundleVersion`; `tauri ios init`
  writes it into `project.yml`. `scripts/app-version.mjs` sets all four with the other files
  ([Releases](#releases)), so a build leaves them as they are. App Store Connect wants a
  `CFBundleVersion` that grows with every upload, of at most three numbers: the release
  ([iOS releases](#ios-releases)) passes Android's versionCode (`node scripts/app-version.mjs --code`) as
  `--config '{"bundle":{"iOS":{"bundleVersion":"<code>"}}}'`, not as `--build-number`, which
  Tauri appends to the version as a fourth number (`0.0.1.1`). Pull requests keep the version.
- The scene lifecycle: `UIApplicationSceneManifest` in `Info.plist` and `project.yml`, one scene
  with tao's `TaoSceneDelegate` (see [On the phone](#on-the-phone)). The template of tauri-cli
  2.12.4 has none. On it iOS hands the Universal Link the app is started with to the scene
  delegate's `scene:willConnectToSession:options:`, which tao 0.37 ignores: `Sources/hashi_tauri/main.mm`
  wraps that method of tao's class before the first scene connects (once UIKit has set the app's
  delegate; a scene restored from an earlier launch never asks for its configuration) and passes
  the launch's links on to `scene:continueUserActivity:` and `scene:openURLContexts:`, where tao
  hands the links of a running app to the deep-link plugin. Checked in the simulator with a test
  scheme: the link that starts the app reaches the page's `get_current`.

The iOS side of the app's own plugin `hashi` is outside `gen/apple`, in `src-tauri/ios/`: a Swift
package (`HashiPlugin.swift`, the counterpart of `HashiPlugin.kt`) that `build.rs` builds with
`swift build` and links into the app's library when it builds for iOS, as `tauri-plugin` does for a
plugin crate's `ios/` (an app's own plugin can't use that). It imports Tauri's Swift API, which
`build.rs` copies from the `tauri` crate to `src-tauri/.tauri/` (ignored by git); Swift Package
Manager fetches its one dependency, swift-rs, from GitHub. It builds for the iOS of
`IPHONEOS_DEPLOYMENT_TARGET`, which Xcode sets to the app's minimum, and
`src-tauri/.cargo/config.toml` to the same outside Xcode (CI's Clippy): without it the package was
built for iOS 13, where GameKit lacks what the plugin uses.

`bundle.iOS.minimumSystemVersion` in `tauri.conf.json` is **iOS 16.4**: the app's page runs in the
system's `WKWebView`, the Safari of the phone's iOS. Tailwind 4 supports Safari 16.4 and newer
(`@property`, `color-mix()`, cascade layers), and the stylesheet uses container queries (Safari 16)
and `:has()` (15.4). The WASM needs less, Safari 15 (reference types, bulk memory, non-trapping
float-to-int; `wasm-tools validate --features` on the bundle). iOS 16 runs on the iPhone 8 and
newer. Raise it with the CSS or the Rust toolchain's WASM features, never lower it below them.

The page runs at `tauri://localhost` on iOS (`useHttpsScheme` is Android's), the server's CORS list
has it; its IPC is `ipc:`, which the content security policy allows. Like Android's origin, it can
never change after the first release.

### Building and running

On a Mac with Xcode 26 (the App Store takes builds with the iOS 26 SDK or newer only), from
`app/src-tauri/`:

```sh
rustup target add aarch64-apple-ios aarch64-apple-ios-sim
cargo tauri ios build --debug --target aarch64-sim --no-sign   # gen/apple/build/arm64-sim/Hashi.app
xcrun simctl install booted gen/apple/build/arm64-sim/Hashi.app && xcrun simctl launch booted rs.hashi
cargo tauri ios dev                                            # a simulator, with the dev server
```

`--no-sign` builds without a team or a certificate, enough for the simulator. Xcode's build phase
"Build Rust Code" calls `cargo tauri ios xcode-script`, which builds the library for the
architecture Xcode asks for. The development team is `Q35TCGM5PW` (`bundle.iOS.developmentTeam`),
which Tauri writes into the Xcode project as `DEVELOPMENT_TEAM` on every build (committed, so a
build leaves the project as it is). It signs nothing without a certificate: the release workflow
signs with the team's App Store Connect API key ([iOS releases](#ios-releases)). After a Tauri update, compare
`gen/apple` with a fresh `cargo tauri ios init --ci` (on a Mac, or in a temporary CI step that
uploads the directory as an artifact): the template changes with Tauri.

### In CI

"Build app (iOS)" in `.github/workflows/ci.yml` runs on `macos-26` (Apple silicon) with Xcode 26.6
pinned (`DEVELOPER_DIR`; the release uses the same). Like the Android build it takes the bundle
of "Build app (web)" and skips `beforeBuildCommand`. It runs Clippy for `aarch64-apple-ios`, builds
the debug app for the simulator and checks that the build left `gen/apple` as committed and that the
icons have no alpha channel. The app is the artifact `ios-simulator-app`, its size in the job
summary. Then it runs the [simulator test](#simulator-test) on an iPhone 17 simulator of iOS 26.5,
booted while the app builds, with hashi.rs blocked on the runner. Not a required check.

## Simulator test

`e2e/ios/smoke.spec.ts`, with its own Playwright config (`e2e/ios.config.ts`), the counterpart of
the [emulator test](#emulator-test): the debug app on an iOS simulator, its page driven through
[Appium](https://appium.io)'s XCUITest driver. Playwright can't attach to a `WKWebView`; Appium
reaches it through the simulator's WebKit inspector (Safari's Web Inspector), which debug builds
allow (wry sets `isInspectable` with `debug_assertions`); the inspector lists it as `process-Hashi`,
the name of the app's process, not by its bundle ID (the session's `additionalWebviewBundleIds`).
Playwright Test still runs the tests, reports and retries like the Android ones;
`e2e/ios/webdriver.ts` sends Appium the few WebDriver commands they need: scripts in the page
(`execute`, which awaits a promise), clicks on its elements (WebDriver's click, a user's gesture),
the driver's `mobile:` commands (clear, launch and activate an app, the app in front) and the page's
console from the inspector.

Each test clears the app's data and launches it (`mobile: clearApp`, `mobile: launchApp`), selects
its page once the inspector lists it and waits for it to hydrate. Another page of the app is opened
with WebDriver's navigation, which waits for the load event: a script sent while the document
changes waits for the next one. WebDriver can't run a script before the page's own like Playwright's
`addInitScript`: after every page load the test adds listeners for errors and
`securitypolicyviolation` events and notes the page's `fetch`es, and the console of the inspector
(Appium's `safariConsole` log) covers the start of the page. Any error, CSP message or answer from
hashi.rs fails the test.

What it checks: the app starts on its start page at `tauri://localhost`, with Tauri's bridge, an
iPhone's user agent (`x-hashi-app: ios/...`) and the daily's offline state; no page breaks the
content security policy (`/`, `/rules`, `/ja/rules`, `/practice`, `/create`, `/lab`, `/dojo`,
`/groups`, `/daily`); practice generates through the native generator (Tauri's IPC is a fetch of
`ipc://localhost/generate_puzzle` on iOS) and hints and checks without the server; the ranks show
one offline notice, again after an `online` event; a link to GitHub and a `location` to another site
leave the app's page where it is and bring Safari to the front; the practice page clears the safe
areas (step 13c): the header starts below `env(safe-area-inset-top)` and the tab bar's labels end
above the home indicator, in both themes, and turned sideways (WebDriver's orientation) the top's
inset is 0, the body's and the tab bar's sides clear the Dynamic Island, as the scene lifecycle
gives them; the app's own plugin answers `system_bars` and `share` with a file (without its Swift
side Tauri refuses them); Game Center's sign-in answers (available, nobody signed in) with nothing
of GameKit over the page, a report goes through, and the button's command shows the sign-in sheet
GameKit kept, or is refused without one. The screenshots `start.png`, `practice.png`,
`practice-light.png`, `practice-dark.png` (the page dark on a light phone: the status bar's icons
light), `landscape.png`, `share.png` (the share sheet with the file) and `game-center.png` are the
artifact
`ios-simulator-screenshot`.

Not checked, unlike on Android: `navigator.onLine` (the simulator has the Mac's network, only
hashi.rs is blocked), the back button (iOS has none; the swipe from the edge isn't a WebDriver
gesture), App Links (Universal Links need a signed build), the copy button (a click on it froze the
simulator's page for 25 s, the clipboard's sync with the Mac), the status bar's style (the
screenshots show it) and the soft keyboard (the simulator may have the Mac's keyboard connected).

The debug build calls the production server when it can. The simulator has no airplane mode: the
global setup refuses to run while hashi.rs answers from the Mac, and CI points hashi.rs to `0.0.0.0`
in `/etc/hosts`.

In CI, after the build: Appium (`APPIUM_VERSION`, `XCUITEST_VERSION` in the job, installed with npm)
and the driver's prebuilt WebDriverAgent for simulators (`download-wda --kind sim`; building it with
Xcode takes minutes), then the tests. A simulator is flaky like an emulator: the config retries a
failed test once, and the job summary counts the flaky ones next to the device, the times of the
build, Appium's setup and the tests, and each test's launch time. A failed run also uploads
`ios-simulator-report`: the Playwright report (with each test's console), Appium's log and the
simulator's log of the app and WebKit.

On a Mac with Xcode, one booted simulator and the app built for it ([Building and
running](#building-and-running-1)):



```sh
npm install --global appium@3.8.0 && appium driver install xcuitest@12.20.0
sudo sh -c 'printf "0.0.0.0 hashi.rs\n:: hashi.rs\n" >> /etc/hosts'   # undo it afterwards
pnpm run app:ios-e2e
```

The global setup installs the app (`E2E_IOS_APP=<path>` for another build), starts Appium unless
one runs at `E2E_APPIUM_URL` (`http://127.0.0.1:4723`) and writes its log to
`e2e/test-results-ios-appium.log`. Without `E2E_WDA` (the `.app` of `appium driver run xcuitest
download-wda -- --kind sim --platform iOS --outdir <dir>`) Appium builds WebDriverAgent with Xcode
on the first run. `E2E_SIMULATOR=<udid>` picks a simulator when more than one is booted. To look
at the page while a test runs: Safari, Develop, the simulator, then `localhost`.
