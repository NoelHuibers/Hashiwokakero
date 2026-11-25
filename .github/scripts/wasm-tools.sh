#!/usr/bin/env bash
# wasm-bindgen and wasm-opt for the WASM builds, in ~/.cache/wasm-tools/bin and on the PATH of the
# job's later steps. cargo-leptos and Trunk use them from there instead of downloading their own
# during the build (cargo-leptos any on the PATH, Trunk the ones of its pinned versions): a
# download that failed there failed the build, after minutes of compiling. Here they come from the
# job's cache (actions/cache, keyed on the versions) or are downloaded with retries.
#
#   .github/scripts/wasm-tools.sh cargo-leptos key       the cache key, as `key` of the step's outputs
#   .github/scripts/wasm-tools.sh cargo-leptos install   from the cache, or downloaded
#
# For cargo-leptos or Trunk: wasm-bindgen is Cargo.lock's version for both, wasm-opt the one the
# tool would download, cargo-leptos 0.3.11's default or wasm_opt of app/Trunk.toml. Linux x86_64,
# where both builds run.
set -euo pipefail

tool=${1:-}
mode=${2:-}
dir="$HOME/.cache/wasm-tools"

bindgen=$(awk '$0 == "name = \"wasm-bindgen\"" { getline; gsub(/^version = "|"$/, ""); print; exit }' Cargo.lock)
case "$tool" in
  # cargo-leptos 0.3.11 asks GitHub for the newest version first (a 504 there once), then takes
  # its default anyway
  cargo-leptos) opt=version_123 ;;
  trunk) opt=$(sed -n 's/^wasm_opt = "\(version_[0-9]*\)"$/\1/p' app/Trunk.toml) ;;
  *) echo "usage: $0 cargo-leptos|trunk key|install" >&2; exit 2 ;;
esac
if [ -z "$bindgen" ] || [ -z "$opt" ]; then
  echo "::error::no wasm-bindgen version in Cargo.lock or no wasm-opt version for $tool"
  exit 1
fi

case "$mode" in
  key)
    echo "key=wasm-tools-$(uname -s)-$(uname -m)-wasm-bindgen-$bindgen-wasm-opt-$opt" >> "$GITHUB_OUTPUT"
    exit 0
    ;;
  install) ;;
  *) echo "usage: $0 cargo-leptos|trunk key|install" >&2; exit 2 ;;
esac

if [ "$(uname -s)-$(uname -m)" != Linux-x86_64 ]; then
  echo "::error::wasm-tools.sh only knows the downloads for Linux x86_64"
  exit 1
fi

# The version each one reports, as Trunk reads it: "wasm-bindgen 0.2.130", "wasm-opt version 123 (version_123)"
have() { "$dir/bin/$1" --version 2> /dev/null | awk -v field="$2" '{ print $field }' || true; }

# Into a temporary file first: an interrupted download leaves no half a binary behind
fetch() {
  local url=$1 member=$2 strip=$3 archive
  archive=$(mktemp)
  echo "Downloading $url"
  .github/scripts/retry.sh curl -fsSL --proto '=https' --tlsv1.2 --retry 3 --retry-all-errors --retry-delay 5 -o "$archive" "$url"
  tar -xzf "$archive" -C "$dir/bin" --strip-components "$strip" "$member"
  rm -f "$archive"
}

mkdir -p "$dir/bin"
if [ "$(have wasm-bindgen 2)" = "$bindgen" ]; then
  echo "wasm-bindgen $bindgen from the cache"
else
  fetch "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/$bindgen/wasm-bindgen-$bindgen-x86_64-unknown-linux-musl.tar.gz" \
    "wasm-bindgen-$bindgen-x86_64-unknown-linux-musl/wasm-bindgen" 1
fi
if [ "$(have wasm-opt 3)" = "${opt#version_}" ]; then
  echo "wasm-opt $opt from the cache"
else
  fetch "https://github.com/WebAssembly/binaryen/releases/download/$opt/binaryen-$opt-x86_64-linux.tar.gz" \
    "binaryen-$opt/bin/wasm-opt" 2
fi

# What the build will use
[ "$(have wasm-bindgen 2)" = "$bindgen" ] || { echo "::error::$dir/bin/wasm-bindgen isn't version $bindgen"; exit 1; }
[ "$(have wasm-opt 3)" = "${opt#version_}" ] || { echo "::error::$dir/bin/wasm-opt isn't $opt"; exit 1; }
"$dir/bin/wasm-bindgen" --version
"$dir/bin/wasm-opt" --version
echo "$dir/bin" >> "$GITHUB_PATH"
