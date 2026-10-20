#!/bin/sh
# Builds the proof checkers of the SAT Dojo from pinned sources, each file checked against its
# SHA-256 before it is compiled:
#
# - drat-trim (Marijn Heule): checks DRAT proofs backwards and writes them as LRAT with hints
# - cake_lpr (Yong Kiam Tan et al.): checks LRAT proofs. Compiled by CakeML from a checker
#   that is proven correct in HOL4; the repository ships the generated assembly
#
#   dojo/checkers/build.sh [DIR]    builds DIR/drat-trim and DIR/cake_lpr (default: .)
#
# Needs a C compiler (cc), curl and sha256sum. Used by dojo/Dockerfile and
# dojo/Dockerfile.prebuilt for the worker image and by CI for the tests. Locally:
#   dojo/checkers/build.sh ~/.local/bin
# makes `dojo run` check proofs exactly like the leaderboard.
set -eu

out="${1:-.}"
mkdir -p "$out"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

DRAT_TRIM=2e3b2dc0ecf938addbd779d42877b6ed69d9a985
CAKE_LPR=a36874a8b750b43fe4b385b8ddbf5b033e46a3fa

# fetch REPO COMMIT FILE SHA256
fetch() {
  curl -fsSL --retry 3 -o "$work/$3" "https://raw.githubusercontent.com/$1/$2/$3"
  echo "$4  $work/$3" | sha256sum -c - >/dev/null || {
    echo "checksum mismatch for $1/$3" >&2
    exit 1
  }
}

fetch marijnheule/drat-trim "$DRAT_TRIM" drat-trim.c \
  d834b649f437e091597f5347f259b9f681087f89ca0844d0cee250a1a1a0c2ee
cc -O2 -o "$out/drat-trim" "$work/drat-trim.c"

fetch tanyongkiam/cake_lpr "$CAKE_LPR" basis_ffi.c \
  8e30d84fdcb2177aa5571d7fa6661a2fae5ecfd56baa0ce49c65f9233a9f87cb
case "$(uname -m)" in
  x86_64 | amd64)
    fetch tanyongkiam/cake_lpr "$CAKE_LPR" cake_lpr.S \
      2f3af32d55083839b3fa0e693afd817679c0b8944bef41def05a8b0ec72b7d4a
    asm=cake_lpr.S
    ;;
  aarch64 | arm64)
    fetch tanyongkiam/cake_lpr "$CAKE_LPR" cake_lpr_arm8.S \
      95b64883edc0cb09feedbcb1ebec233e2490f5b458fdda9dc29c212ed916f00c
    asm=cake_lpr_arm8.S
    ;;
  *)
    echo "cake_lpr has no build for $(uname -m)" >&2
    exit 1
    ;;
esac
# basis_ffi.c prints "CakeML heap space exhausted." (exit code 1) when memory runs out
cc -O2 -std=c99 -o "$out/cake_lpr" "$work/basis_ffi.c" "$work/$asm"

echo "built $out/drat-trim and $out/cake_lpr"
