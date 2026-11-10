#!/bin/sh
# Cargo's RUSTC_WRAPPER for the foss build (app/fdroid/build.sh): runs rustc with
# CARGO_MANIFEST_PATH without the machine's paths. wasm_split_macros (Leptos) hashes that absolute
# path into a symbol of the WASM (_WASM_SPLIT_MARKER_<hash>), so the bundle, and with it the
# APK, differed between our release and F-Droid's build server. The same mapping as the
# --remap-path-prefix flags of build.sh: the checkout is /hashi, Cargo's home /cargo.
case ${CARGO_MANIFEST_PATH:-} in
  "$HASHI_FDROID_ROOT"/*) CARGO_MANIFEST_PATH=/hashi${CARGO_MANIFEST_PATH#"$HASHI_FDROID_ROOT"} ;;
  "$CARGO_HOME"/*) CARGO_MANIFEST_PATH=/cargo${CARGO_MANIFEST_PATH#"$CARGO_HOME"} ;;
esac
export CARGO_MANIFEST_PATH
exec "$@"
