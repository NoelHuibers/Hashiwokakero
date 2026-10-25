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

The smoke test checks the session (a guest made in the app, no cookie, the same guest after a
reload), the headers, one update toast however many calls the server refuses, and the links.

## App Links

Where the app is installed, Android opens links to some pages of `https://hashi.rs` in the app
instead of the browser, the ones people send each other: an invite to a group (`/g/<code>`), the
daily (`/`, where a daily result's share text points), practice (`/practice`, the practice's),
the groups (`/groups`, `/groups/...`) and players' pages (`/player/<id>`), each in every language
(`/ja/g/<code>`). Not the rules, the legal pages, the admin's, the Dojo with its uploads, or
anything the server answers itself (`/api`, `/og`, the downloads). `seo::APP_LINK_PATHS` and
`APP_LINK_PREFIXES` list them, and `plugins.deep-link.mobile` in `src-tauri/tauri.conf.json` for
Android (a test in `seo.rs` compares the two and prints the JSON to paste after a change, a new
language say).

- **The app:** `tauri-plugin-deep-link` (official) writes an intent filter with
  `android:autoVerify="true"` for these paths into the manifest when the app is built (between its
  `DEEP LINK PLUGIN` markers, committed like the rest of `gen/android`). The page asks the plugin
  for the link the app started with (`get_current`, allowed in the capabilities) and listens for
  the next ones (Tauri's event `deep-link://new-url`), and its router shows their page
  (`api::native::follow_app_links`), keeping the language of the link: the one the app started
  with instead of the start page, later ones on top of the page the app was on, which the back
  button goes back to. Other URLs change nothing. The link never becomes a navigation of the
  `WebView`, so the guard that sends other sites to the browser (`on_navigation`) doesn't see it.
  Links to these pages from the app's own pages: there are none (share and invite links are
  copied or shared, not followed); one would go to Android, which brings it back to the app.
- **The server** says which app may open its links: `/.well-known/assetlinks.json` names
  `rs.hashi` and the SHA-256 fingerprints of its signing certificates, from
  `HASHI_ANDROID_CERT_SHA256` (docs/deployment.md; a 404 without it). Android fetches it when the
  app is installed and opens the links in the app only if the installed app's certificate is
  listed. The upload key's is in `fly.toml` (it signs the APK of the GitHub release); the app from
  Play is signed by Google, whose certificate the owner adds once the app is in Play
  (docs/mobile.md, owner tasks). Debug builds (`rs.hashi.debug`) are never verified: the links
  open them only when an intent names the package, as the emulator test's do.

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
  plugin's `get_current`, and the app's own plugin `hashi` (`share`, `system_bars`). `build.rs` lists the commands of the app and of
  its plugin, which makes Tauri check them against the capabilities.
- Plugins: `tauri-plugin-opener` (official, links out of the app, its own click script off),
  `tauri-plugin-deep-link` (official, [App Links](#app-links)), and `hashi`, the app's own, in `src/lib.rs` with its Android side in `gen/android`
  (`HashiPlugin.kt`). See [On the phone](#on-the-phone).
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
  intent filter is the deep-link plugin's), `MainActivity.kt` (the system bars and the keyboard,
  see [On the phone](#on-the-phone)) and `HashiPlugin.kt`, and the template's
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
`android-release.yml`, and `ndkVersion` in `gen/android/app/build.gradle.kts` (Gradle strips
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
event), and Android's back button goes back from the practice to the rules instead of closing
the app.

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

A release is a tag `app-v<x.y.z>` on main. `.github/workflows/android-release.yml` builds the
App Bundle for Google Play from it, signs it with the upload key and uploads it to Play's
internal test track; from there it is promoted in the Play Console (closed test, production).

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

Run by hand (`gh workflow run android-release.yml --ref <ref>`), or by a pull request that
changes the workflow, `gen/android/app/build.gradle.kts` or `scripts/app-version.mjs`, the
workflow is a dry run: it builds, signs and checks, keeps the files as the artifact for three
days, and publishes nothing. On a branch it skips the checks of the tag, of main and of CI.
Without the signing secrets (pull requests of Dependabot or of forks) a dry run signs with a
throwaway key. With `-f publish=true` on a tag
(`--ref app-v0.2.0`) it is the full release again, e.g. to upload a version once the service
account exists; the GitHub release then only gets its files replaced.

### The first release

Play's API can only upload to an app that already has a bundle: the first one has to be
uploaded by hand. Create the app `rs.hashi` in the Play Console, push the first tag (the upload to
Play is skipped or fails), download `hashi-<version>.aab` from its GitHub release and upload it to
the internal test track by hand, accepting Play App Signing (Google keeps the app signing key, the
upload key only signs uploads). From then on, with the service account set up
([docs/mobile.md](../docs/mobile.md#owner-tasks)), tags upload by themselves.

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
  `CFBundleVersion` that grows with every upload, of at most three numbers: the release (step 13e)
  passes Android's versionCode (`node scripts/app-version.mjs --code`) as
  `--config '{"bundle":{"iOS":{"bundleVersion":"<code>"}}}'`, not as `--build-number`, which
  Tauri appends to the version as a fourth number (`0.0.1.1`). Pull requests keep the version.

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
architecture Xcode asks for. No development team is set in the project: the release workflow
(step 13e) will pass it, with an App Store Connect API key. After a Tauri update, compare
`gen/apple` with a fresh `cargo tauri ios init --ci` (on a Mac, or in a temporary CI step that
uploads the directory as an artifact): the template changes with Tauri.

### In CI

"Build app (iOS)" in `.github/workflows/ci.yml` runs on `macos-26` (Apple silicon) with Xcode 26.6
pinned (`DEVELOPER_DIR`; the release will use the same). Like the Android build it takes the bundle
of "Build app (web)" and skips `beforeBuildCommand`. It runs Clippy for `aarch64-apple-ios`, builds
the debug app for the simulator and checks that the build left `gen/apple` as committed and that the
icons have no alpha channel. The app is the artifact `ios-simulator-app`, its size in the job
summary. Then it starts the app on an iPhone 17 simulator of iOS 26.5, booted while the app builds,
with hashi.rs blocked on the runner (no guest is made there; the start page shows its offline
state): the job fails when the app doesn't start or doesn't run ten seconds later. A screenshot of
it then is the artifact `ios-simulator-screenshot`; a failed run also keeps the simulator's log of
the app and WebKit, `ios-simulator-log`. Not a required check.
