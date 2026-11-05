#!/usr/bin/env bash
# The foss APKs built from source, the same way on F-Droid's build server and in our release
# (app/README.md, F-Droid). F-Droid's recipe (app/fdroid/fdroiddata/rs.hashi.yml) runs:
#
#   app/fdroid/build.sh prepare     prebuild: Node, pnpm and the packages, Rust, every crate, and
#                                   the Cargo tools (Trunk, wasm-bindgen, Tauri) compiled: every
#                                   download but Gradle's
#   app/fdroid/build.sh tools       build: Tailwind's two native modules and wasm-opt, compiled from
#                                   the submodules in third_party/
#   app/fdroid/build.sh apk arm64   build: the APK of that ABI, unsigned
#
# The release (app-release.yml, "Build APKs (Android, foss)") runs only `apk arm64 arm x86_64`,
# with CI's tools and the direct key in the environment (ANDROID_KEYSTORE_FILE, ...). Both build
# with the same Rust, wasm-bindgen, wasm-opt, Tailwind, NDK and Gradle, and without the machine's
# paths in the binaries: F-Droid's APK is ours but for the signature, and F-Droid ships ours
# (reproducible builds, `binary:` in the recipe).
#
# Versions: Rust, the Tauri CLI and Node are pinned below; Trunk and wasm-opt in app/Trunk.toml,
# wasm-bindgen in Cargo.lock, pnpm in package.json, Tailwind and Lightning CSS in pnpm-lock.yaml.
# third_party/ has the sources of the last three, at the same versions (`check` compares them,
# CI runs it): after an update of Tailwind (`@tailwindcss/oxide`), Lightning CSS or wasm-opt,
#
#   git -C app/fdroid/third_party/tailwindcss fetch --tags && git -C ... checkout v4.3.4
#
# and commit the submodule.
set -euo pipefail

# rustc of the foss build. The release's foss APKs and F-Droid's build use this one, so they
# are the same; the other builds take the current stable (rust-toolchain.toml).
RUST_VERSION=1.99.1
# As in the workflows (cargo binstall tauri-cli@...), checked by `apk`
TAURI_CLI_VERSION=2.12.4
# .node-version's major, from nodejs.org with its checksum (SHASUMS256.txt of the release)
NODE_VERSION=26.13.0
NODE_SHA256=d28a96af6c38596f6d9d97c0ee531c13d238d757742387d1928de822d959e997

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
TOOLS=${HASHI_FDROID_TOOLS:-$HOME/.local/hashi-fdroid}
TARGETS=(aarch64-linux-android armv7-linux-androideabi x86_64-linux-android)
export CARGO_HOME=${CARGO_HOME:-$HOME/.cargo}
export PATH="$TOOLS/node/bin:$TOOLS/bin:$CARGO_HOME/bin:$PATH"
export RUSTUP_TOOLCHAIN=$RUST_VERSION
export CARGO_TERM_COLOR=${CARGO_TERM_COLOR:-never}

fail() {
  echo "fdroid/build.sh: $*" >&2
  exit 1
}

# The version of a package in a Cargo.lock
lock_version() {
  awk -v name="$2" '$0 == "name = \"" name "\"" { getline; gsub(/version = |"/, ""); print; exit }' "$1"
}
# The version of a package in pnpm-lock.yaml (its first entry in `packages:`)
pnpm_version() {
  sed -n "s|^  '\{0,1\}$1@\([0-9][^':]*\)'\{0,1\}:\$|\1|p" "$ROOT/pnpm-lock.yaml" | head -n 1
}
trunk_version() { sed -n 's/^trunk-version = "=\(.*\)"$/\1/p' "$ROOT/app/Trunk.toml"; }
wasm_opt_version() { sed -n 's/^wasm_opt = "version_\(.*\)"$/\1/p' "$ROOT/app/Trunk.toml"; }
pnpm_pinned() { sed -n 's/^  "packageManager": "pnpm@\(.*\)",$/\1/p' "$ROOT/package.json"; }
# The pinned Rust with the targets of the bundle and the APKs (rustup only goes online for what
# is missing)
toolchain() {
  local installed target
  installed=$(rustup target list --installed --toolchain "$RUST_VERSION" 2> /dev/null || true)
  for target in wasm32-unknown-unknown "${TARGETS[@]}"; do
    if ! grep -qx "$target" <<< "$installed"; then
      rustup toolchain install "$RUST_VERSION" --profile minimal \
        --target "$(IFS=,; echo "wasm32-unknown-unknown,${TARGETS[*]}")"
      return
    fi
  done
}
json_version() { sed -n 's/^  "version": "\(.*\)",$/\1/p' "$1" | head -n 1; }

# Node and pnpm (not in Debian in these versions), the packages of pnpm-lock.yaml (Tailwind, the
# fonts), Rust with the targets, every crate of both lock files, and the Cargo tools, compiled.
# Everything that downloads, so `tools` and `apk` work offline (but for Gradle's dependencies).
prepare() {
  mkdir -p "$TOOLS/bin"
  if [ "$(node --version 2> /dev/null)" != "v$NODE_VERSION" ]; then
    local tarball="$TOOLS/node-v$NODE_VERSION-linux-x64.tar.xz"
    curl -sSfL -o "$tarball" "https://nodejs.org/dist/v$NODE_VERSION/node-v$NODE_VERSION-linux-x64.tar.xz"
    echo "$NODE_SHA256  $tarball" | sha256sum -c --quiet
    rm -rf "$TOOLS/node" && mkdir "$TOOLS/node"
    tar -xJf "$tarball" -C "$TOOLS/node" --strip-components=1
    rm "$tarball"
  fi
  local pnpm
  pnpm=$(pnpm_pinned)
  [ -n "$pnpm" ] || fail "no pnpm version in package.json's packageManager"
  if [ "$(pnpm --version 2> /dev/null)" != "$pnpm" ]; then
    npm install --global --no-fund --no-audit "pnpm@$pnpm"
  fi
  # Dependencies' install scripts don't run (pnpm-workspace.yaml allows none); ours writes
  # public/fonts from the @fontsource packages
  (cd "$ROOT" && pnpm install --frozen-lockfile)

  toolchain
  cargo fetch --locked --manifest-path "$ROOT/Cargo.toml"
  cargo fetch --locked --manifest-path "$ROOT/app/src-tauri/Cargo.toml"
  # The crates of the native modules that `tools` builds
  cargo fetch --locked --manifest-path "$ROOT/app/fdroid/third_party/tailwindcss/Cargo.toml"
  cargo fetch --locked --manifest-path "$ROOT/app/fdroid/third_party/lightningcss/Cargo.toml"
  cargo install --locked --root "$TOOLS" trunk --version "$(trunk_version)"
  cargo install --locked --root "$TOOLS" wasm-bindgen-cli --version "$(lock_version "$ROOT/Cargo.lock" wasm-bindgen)"
  cargo install --locked --root "$TOOLS" tauri-cli --version "$TAURI_CLI_VERSION"
}

THIRD_PARTY=$ROOT/app/fdroid/third_party
# The submodules and the versions they have to be at: name, tag
submodules() {
  echo "tailwindcss v$(pnpm_version @tailwindcss/oxide)"
  echo "lightningcss v$(pnpm_version lightningcss)"
  echo "binaryen version_$(wasm_opt_version)"
}

# The submodules are the versions of the packages and of Trunk.toml: each one's commit is its
# tag's on its upstream (git ls-remote, nothing is cloned)
check() {
  local name tag path url want have
  while read -r name tag; do
    path=app/fdroid/third_party/$name
    url=$(git -C "$ROOT" config -f .gitmodules "submodule.$path.url")
    have=$(git -C "$ROOT" ls-files --stage -- "$path" | awk '{ print $2 }')
    # An annotated tag's commit is the peeled ref (^{}), listed after the tag itself
    want=$(git ls-remote "$url" "refs/tags/$tag" "refs/tags/$tag^{}" | awk '{ sha = $1 } END { print sha }')
    [ -n "$want" ] || fail "$url has no tag $tag"
    [ "$have" = "$want" ] || fail "$path is at ${have:-nothing}, not at $tag ($want): git -C $path fetch --tags && git -C $path checkout $tag, then commit it"
    echo "$path: $tag"
  done < <(submodules)
}

# From the submodules' sources: Tailwind's scanner (oxide) and Lightning CSS, the two native
# modules of the Tailwind CLI, built here instead of their prebuilt npm packages (F-Droid's
# scanner deletes those, scandelete: node_modules); and binaryen's wasm-opt, which Trunk runs.
tools() {
  local want have
  want=$(pnpm_version @tailwindcss/oxide) have=$(json_version "$THIRD_PARTY/tailwindcss/packages/tailwindcss/package.json")
  [ "$want" = "$have" ] || fail "third_party/tailwindcss is ${have:-missing}, pnpm-lock.yaml has @tailwindcss/oxide $want"
  want=$(pnpm_version lightningcss) have=$(json_version "$THIRD_PARTY/lightningcss/package.json")
  [ "$want" = "$have" ] || fail "third_party/lightningcss is ${have:-missing}, pnpm-lock.yaml has $want"
  want=$(wasm_opt_version)
  have=$(sed -n 's/^project(binaryen.* VERSION \([0-9]*\)).*$/\1/p' "$THIRD_PARTY/binaryen/CMakeLists.txt" 2> /dev/null || true)
  [ -n "$want" ] && [ "$want" = "$have" ] || fail "third_party/binaryen is ${have:-missing}, app/Trunk.toml has wasm_opt version_$want"

  (cd "$THIRD_PARTY/tailwindcss" && cargo build --release --locked --offline -p tailwind-oxide)
  (cd "$THIRD_PARTY/lightningcss" && cargo build --release --locked --offline -p lightningcss_node)
  # In the platform packages' places, where the prebuilt ones were
  local oxide css
  oxide=$(find "$ROOT/node_modules/.pnpm" -path '*/node_modules/@tailwindcss/oxide-linux-x64-gnu' -type d | head -n 1)
  css=$(find "$ROOT/node_modules/.pnpm" -path '*/node_modules/lightningcss-linux-x64-gnu' -type d | head -n 1)
  [ -n "$oxide" ] && [ -n "$css" ] || fail "no linux-x64-gnu packages of oxide and lightningcss: run prepare first"
  cp "$THIRD_PARTY/tailwindcss/target/release/libtailwind_oxide.so" "$oxide/tailwindcss-oxide.linux-x64-gnu.node"
  cp "$THIRD_PARTY/lightningcss/target/release/liblightningcss_node.so" "$css/lightningcss.linux-x64-gnu.node"

  cmake -S "$THIRD_PARTY/binaryen" -B "$TOOLS/binaryen-build" -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_TESTS=OFF -DBUILD_SHARED_LIBS=OFF -DENABLE_WERROR=OFF
  cmake --build "$TOOLS/binaryen-build" --target wasm-opt --parallel "${CARGO_BUILD_JOBS:-$(nproc)}"
  install -m 755 "$TOOLS/binaryen-build/bin/wasm-opt" "$TOOLS/bin/wasm-opt"
}

# The foss APKs of the ABIs (arm64, arm, x86_64), release, from app/src-tauri, with the web bundle
# (Tauri's beforeBuildCommand, Trunk). Signed when ANDROID_KEYSTORE_FILE and the rest are set
# (gen/android/app/build.gradle.kts), else unsigned (F-Droid).
apk() {
  [ $# -gt 0 ] || fail "usage: apk <abi>... (arm64, arm, x86_64)"
  local args=() abi
  for abi in "$@"; do
    case $abi in
      arm64) args+=(aarch64) ;;
      arm) args+=(armv7) ;;
      x86_64) args+=(x86_64) ;;
      *) fail "unknown ABI $abi (arm64, arm, x86_64)" ;;
    esac
  done
  local tauri
  tauri=$(cargo tauri --version 2> /dev/null | awk '{ print $2 }')
  [ "$tauri" = "$TAURI_CLI_VERSION" ] || fail "tauri-cli $TAURI_CLI_VERSION needed, not '${tauri}'"
  toolchain

  export NDK_HOME=${NDK_HOME:-${ANDROID_NDK_HOME:-}}
  [ -d "$NDK_HOME" ] || fail "no NDK: set NDK_HOME (or ANDROID_NDK_HOME)"
  export HASHI_ANDROID_FLAVOR=foss
  # Without the machine's paths in the binaries (panic messages carry the sources' paths): this
  # checkout and Cargo's home differ between CI and F-Droid. Added to Tauri's own flags of the
  # Android targets (app/src-tauri/.cargo/config.toml, which Cargo merges with these).
  local remap="--remap-path-prefix=$ROOT=/hashi --remap-path-prefix=$CARGO_HOME=/cargo"
  local triple
  for triple in wasm32-unknown-unknown "${TARGETS[@]}"; do
    triple=${triple^^}
    export "CARGO_TARGET_${triple//-/_}_RUSTFLAGS=$remap"
  done
  # Whatever reads it (the build's own outputs have no time in them), the same on both sides
  SOURCE_DATE_EPOCH=$(git -C "$ROOT" log -1 --format=%ct 2> /dev/null || echo 0)
  export SOURCE_DATE_EPOCH
  # F-Droid's scanner removes the Gradle wrapper: its gradlew-fdroid (`gradle`) takes the version
  # of gradle-wrapper.properties and checks the download against its own checksums
  local android="$ROOT/app/src-tauri/gen/android"
  if [ ! -e "$android/gradlew" ]; then
    command -v gradle > /dev/null || fail "neither gradlew nor gradle"
    # shellcheck disable=SC2016 # expanded by the shim
    printf '#!/bin/sh\ncd "$(dirname "$0")" && exec gradle "$@"\n' > "$android/gradlew"
    chmod +x "$android/gradlew"
  fi
  # Trunk takes wasm-bindgen and wasm-opt from the PATH when they are the pinned versions. CI:
  # it downloads them. F-Droid: `tools` built them, and nothing downloads any more (but Gradle)
  if [ -x "$TOOLS/bin/wasm-opt" ]; then
    export TRUNK_OFFLINE=true CARGO_NET_OFFLINE=true
  fi
  (cd "$ROOT/app/src-tauri" && cargo tauri android build --ci --apk --split-per-abi --target "${args[@]}")
}

case ${1:-} in
  prepare | tools | apk | check)
    command=$1
    shift
    "$command" "$@"
    ;;
  *) fail "usage: build.sh prepare | tools | apk <abi>... | check" ;;
esac
