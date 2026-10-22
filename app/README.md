# The app's web bundle

`hashi_app` renders the whole UI of the website in the browser, without a server rendering the
page first (client-side rendering, CSR). It is the web part of the mobile app; the plan and the
steps are in [docs/mobile.md](../docs/mobile.md). The Tauri project that wraps it for Android and
iOS will live in `app/src-tauri/`, a crate of its own outside the workspace.

The crate is a WASM binary whose `main` calls `hashi::mount()`. It depends on `hashi` with the
`csr` feature only; `hashi` with `hydrate` or `ssr` (the website) never gets `csr` from it,
because Cargo only unifies the features of the packages it builds (`-p hashi` doesn't build
this crate).

## Files

- `index.html`: the static page, Trunk's input. It brings what `app::shell` in `src/app.rs`
  adds around `<App/>` on the website (see below).
- `Trunk.toml`: where the bundle goes, the dev server and its `/api` proxy, and two hooks: the
  Tailwind CLI from the pnpm packages (`style/tailwind.css`, run in the repository root like
  cargo-leptos runs it for the website) and a copy of `public/` to the root of the bundle.
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

The server functions call `/api/...` on the page's origin for now; the app's client makes the
server configurable (docs/mobile.md, step 6).

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
