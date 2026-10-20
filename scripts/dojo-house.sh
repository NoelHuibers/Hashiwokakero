#!/bin/sh
# Uploads the Dojo's house entries to the site: the reference solvers (dojo/reference, MiniSat,
# CaDiCaL and Kissat as committed) and our own solvers (dojo/house, built here for
# wasm32-wasip1). Needs the site's HASHI_ADMIN_TOKEN in the environment.
#
#   HASHI_ADMIN_TOKEN=... scripts/dojo-house.sh [URL]     (default https://hashi.rs)
#
# A solver that didn't change is a duplicate and stays as it is; a changed one replaces the
# house entry of the same name (see POST /dojo/house in src/server/dojo/submit.rs). So this can
# run after every deploy: CI does, when the HASHI_ADMIN_TOKEN secret is set. Needs curl 7.87 or
# newer (--url-query).
set -eu

url="${1:-https://hashi.rs}"
: "${HASHI_ADMIN_TOKEN:?set HASHI_ADMIN_TOKEN to the site's admin token}"
root="$(cd "$(dirname "$0")/.." && pwd)"

cargo build --locked -p house --release --target wasm32-wasip1 --manifest-path "$root/Cargo.toml"
house="$root/target/wasm32-wasip1/release"

failed=0
# upload FILE NAME SOURCE DESCRIPTION
upload() {
  status="$(curl -sS -o "${TMPDIR:-/tmp}/dojo-house.out" -w '%{http_code}' -X POST \
    -H "Authorization: Bearer $HASHI_ADMIN_TOKEN" -H "Content-Type: application/wasm" \
    --data-binary "@$1" --url-query "name=$2" --url-query "source=$3" \
    --url-query "description=$4" "$url/dojo/house")"
  body="$(cat "${TMPDIR:-/tmp}/dojo-house.out")"
  case "$status:$body" in
    201:*) echo "$2: uploaded as entry #$body" ;;
    422:duplicate) echo "$2: unchanged" ;;
    *)
      echo "$2: failed with HTTP $status $body" >&2
      failed=1
      ;;
  esac
}

ref="$root/dojo/reference"
upload "$ref/minisat.wasm" "MiniSat" dojo/reference \
  "MiniSat (Eén and Sörensson), the last commit of its repository, with DRAT proofs. The SAT Competition's yardstick."
upload "$ref/cadical.wasm" "CaDiCaL 3.0.1" dojo/reference \
  "CaDiCaL 3.0.1 (Biere et al.) with LRAT proofs, compiled with wasi-sdk."
upload "$ref/kissat.wasm" "Kissat 4.0.4" dojo/reference \
  "Kissat 4.0.4 (Biere et al.) with DRAT proofs, compiled with wasi-sdk."
upload "$house/dpll.wasm" "Hashi DPLL" dojo/house "The DPLL solver behind hashi.rs."
upload "$house/cdcl.wasm" "Hashi CDCL" dojo/house "The CDCL solver behind hashi.rs."
upload "$house/splr.wasm" "splr" dojo/house "splr, a modern CDCL solver written in Rust."
exit $failed
