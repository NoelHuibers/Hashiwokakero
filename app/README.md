# The mobile app

`hashi_app` renders the whole UI of the website in the browser, without a server rendering the
page first (client-side rendering, CSR). It is the web part of the mobile app; the plan and the
steps are in [docs/mobile.md](../docs/mobile.md). The Tauri project that wraps it for Android
(and later iOS) is `app/src-tauri/`, a crate of its own outside the workspace, see
[Android](#android) below.

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

Left for the Android polish (step 10): links with `target="_blank"` (the store's page, the
website) need Tauri's opener plugin, the clipboard and `navigator.share` may need its plugins,
and Blob downloads (the CNF of a puzzle) need a download handler.

The smoke test checks the session (a guest made in the app, no cookie, the same guest after a
reload), the headers, one update toast however many calls the server refuses, and the links.

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

CI runs it in "Smoke test app (web)" on the bundle of "Build app (web)", whose artifact
`app-web` the Android build uses. Neither is a required check.

## Android

`src-tauri/` is a Tauri 2 project (crate `hashi_tauri`, Tauri 2.12, tauri-cli 2.12.4) that shows
the bundle in the system WebView. It has its own `Cargo.lock` and is excluded from the workspace
(`exclude` in the root `Cargo.toml`), so the website's builds never compile Tauri. It still builds
inside the repository, where `.cargo/config.toml` applies, which matters once it calls `backend`
or `hashi` natively (docs/mobile.md, step 9).

- `src/lib.rs`: `run`, the app's entry point on the phones (`tauri::mobile_entry_point`), and
  `src/main.rs`, the same on the desktop, for development only.
- `tauri.conf.json`: application ID `rs.hashi`, the name, the version, the bundle
  (`frontendDist`), the one window, and the content security policy (below).
- `capabilities/default.json`: what the page may call in Tauri: `core:default`, no plugins yet.
- `icons/`: generated from `public/icon-512.png` by `cargo tauri icon app-icon.json`, along with
  the Android launcher icons in `gen/android/.../res/mipmap-*`. Android's adaptive icon cuts a
  circle (or a squircle) out of the middle of its foreground, so the foreground,
  `icons/android-foreground.png`, is the badge alone, small on a transparent square; the
  background is the website's `#f4ecdd`. Made with ImageMagick, in the repository root:

  ```sh
  magick public/icon-512.png -fill none -draw "color 0,0 floodfill" \
    -gravity center -background none -extent 756x756 app/src-tauri/icons/android-foreground.png
  ```

- `gen/android/`: the Android project (Gradle), generated by `cargo tauri android init` and
  committed with its Gradle wrapper. Changed by hand: the window's colours (the website's
  background, light and dark, so the screen doesn't flash white before the page paints), the
  icons, and the checksum of the Gradle download. Build outputs and the files Tauri writes on
  every build are in its `.gitignore`s.

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
library for Android's 16 KB memory pages, which Google Play requires. The web bundle needs what
[Building](#building) lists.

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
pull requests build. The store's App Bundle (step 11) should add `armv7` (older and cheap 32-bit
phones) and `x86_64` (Chromebooks, emulators): Play delivers only the device's own, so more of
them only make the upload bigger. `i686` isn't worth it.

### In CI

"Build app (Android)" in `.github/workflows/ci.yml` takes the bundle of "Build app (web)" from
its artifact `app-web` and skips `beforeBuildCommand`
(`--config '{"build":{"beforeBuildCommand":null}}'`), so the WASM is built once. It runs Clippy
on the crate for Android, then builds the debug APK for arm64 without debug info and uploads it
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
`major × 1,000,000 + minor × 1,000 + patch`, so 0.1.0 is 1000 and 1.2.3 is 1,002,003 (minor and
patch stay below 1000). The release workflow (step 11) sets the version from the tag.
