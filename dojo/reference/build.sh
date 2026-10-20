#!/bin/sh
# Builds the reference solvers of the SAT Dojo from pinned sources, every download checked
# against its SHA-256:
#
# - MiniSat (Eén and Sörensson), the last commit of its repository (2013, after 2.2.0), with
#   a DRAT proof added (patches/minisat.patch): the SAT Competition's yardstick for "easy"
# - CaDiCaL 3.0.1 (Biere et al.), writing LRAT proofs
# - Kissat 4.0.4 (Biere et al.), writing DRAT proofs
#
# Each gets a small main (main/) that runs the solver's own command line the Dojo's way: DIMACS
# on stdin, the answer on stdout, the proof to the file named by the first argument. The
# patches only change what WASI lacks (signals, alarms, pipes to compressors, rusage fields,
# C++ exceptions) and let Kissat read stdin while writing a proof. Flags are the ones the
# solvers' own build scripts use by default (-O3 -DNDEBUG).
#
#   dojo/reference/build.sh             builds dojo/reference/{minisat,cadical,kissat}.wasm with
#                                       wasi-sdk, which it downloads (pinned)
#   dojo/reference/build.sh --check     builds them in a temp directory and compares them with
#                                       the committed ones and SHA256SUMS (CI does this)
#   dojo/reference/build.sh --native DIR
#                                       builds the same sources natively with cc and c++ (the
#                                       calibration study, see dojo/CALIBRATION.md)
#
# Downloads are kept in DOJO_REFERENCE_CACHE (default ~/.cache/dojo-reference). Needs curl,
# tar, patch (or git), sha256sum and, for --native, a C and C++ compiler (CC, CXX). The wasm builds are
# reproducible: the same bytes on every machine.
set -eu
# The same order of object files everywhere (globs sort by locale), so the same bytes
export LC_ALL=C

here="$(cd "$(dirname "$0")" && pwd)"
mode=wasm
out="$here"
case "${1:-}" in
  --native)
    mode=native
    out="${2:?usage: build.sh --native DIR}"
    ;;
  --check) mode=check ;;
  "") ;;
  *)
    echo "usage: build.sh [--check | --native DIR]" >&2
    exit 2
    ;;
esac

WASI_SDK=34
WASI_SDK_SHA256=b761e3a0721dbae9c09a0059e5fdb2bf917d1b4a8a7b430fb3b5aafb0984b2c4
MINISAT=37dc6c67e2af26379d88ce349eb9c4c6160e8543
MINISAT_SHA256=3db05b02f91c4b097b7962e523225aa5e6fa9a6c0d42704a170b01b069cdfcfe
CADICAL=3.0.1
CADICAL_SHA256=0a8ea563b5a25f5aa064634814edab45cc0e45111ea0f5d412a565f806fd7e11
KISSAT=4.0.4
KISSAT_SHA256=bfe93eaa6323b48011e4b1fcf74b3f2e20f9de544767e728009e5b2018296193

cache="${DOJO_REFERENCE_CACHE:-$HOME/.cache/dojo-reference}"
mkdir -p "$cache"
# About 2 GB with wasi-sdk unpacked: TMPDIR can point elsewhere than a small /tmp
work="$(mktemp -d "${TMPDIR:-/tmp}/dojo-reference.XXXXXX")"
trap 'rm -rf "$work"' EXIT

# fetch URL FILE SHA256: downloads into the cache once, checks it every time
fetch() {
  if [ ! -f "$cache/$2" ]; then
    curl -fsSL --retry 3 -o "$cache/$2.part" "$1"
    mv "$cache/$2.part" "$cache/$2"
  fi
  echo "$3  $cache/$2" | sha256sum -c - >/dev/null || {
    echo "checksum mismatch for $2 (delete $cache/$2 to download it again)" >&2
    exit 1
  }
}

# unpack FILE NAME: the archive's single top directory as $work/NAME, patched
unpack() {
  mkdir "$work/$2"
  tar -xzf "$cache/$1" -C "$work/$2" --strip-components=1
  if command -v patch >/dev/null; then
    patch -s -p1 -d "$work/$2" <"$here/patches/$2.patch"
  else
    (cd "$work/$2" && git apply "$here/patches/$2.patch")
  fi
}

fetch "https://github.com/niklasso/minisat/archive/$MINISAT.tar.gz" \
  "minisat-$MINISAT.tar.gz" "$MINISAT_SHA256"
fetch "https://github.com/arminbiere/cadical/archive/refs/tags/rel-$CADICAL.tar.gz" \
  "cadical-$CADICAL.tar.gz" "$CADICAL_SHA256"
fetch "https://github.com/arminbiere/kissat/archive/refs/tags/rel-$KISSAT.tar.gz" \
  "kissat-$KISSAT.tar.gz" "$KISSAT_SHA256"
unpack "minisat-$MINISAT.tar.gz" minisat
unpack "cadical-$CADICAL.tar.gz" cadical
unpack "kissat-$KISSAT.tar.gz" kissat

if [ $mode = native ]; then
  CC="${CC:-cc}"
  CXX="${CXX:-c++}"
  target=""
  libs=""
  ext=""
  compiler="$($CC --version | head -n 1)"
else
  case "$(uname -sm)" in
    "Linux x86_64") sdk_arch=x86_64-linux ;;
    "Linux aarch64") sdk_arch=arm64-linux ;;
    *)
      echo "no pinned wasi-sdk for $(uname -sm)" >&2
      exit 1
      ;;
  esac
  # Only the x86_64 archive is pinned: CI and the worker image build on x86_64
  [ $sdk_arch = x86_64-linux ] || {
    echo "the wasi-sdk checksum is pinned for x86_64 Linux only" >&2
    exit 1
  }
  fetch "https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-$WASI_SDK/wasi-sdk-$WASI_SDK.0-$sdk_arch.tar.gz" \
    "wasi-sdk-$WASI_SDK.0-$sdk_arch.tar.gz" "$WASI_SDK_SHA256"
  mkdir "$work/wasi-sdk"
  tar -xzf "$cache/wasi-sdk-$WASI_SDK.0-$sdk_arch.tar.gz" -C "$work/wasi-sdk" --strip-components=1
  CC="$work/wasi-sdk/bin/clang"
  CXX="$work/wasi-sdk/bin/clang++"
  # No exceptions (wasmtime would need the exception-handling proposal); signal() and the
  # process clocks and getpid() emulated by wasi-libc; 8 MiB of stack instead of 64 KiB; no names or paths
  # in the output, so the bytes don't depend on where it was built
  target="--target=wasm32-wasip1 -fno-exceptions -D_WASI_EMULATED_SIGNAL -D_WASI_EMULATED_PROCESS_CLOCKS -D_WASI_EMULATED_GETPID -ffile-prefix-map=$work=/build -ffile-prefix-map=$here=/reference"
  libs="-lwasi-emulated-signal -lwasi-emulated-process-clocks -lwasi-emulated-getpid -Wl,-z,stack-size=8388608 -Wl,--strip-all"
  ext=".wasm"
  compiler="wasi-sdk $WASI_SDK"
fi
export SOURCE_DATE_EPOCH=0
mkdir -p "$work/out"

# compile COMPILER FLAGS OBJDIR SOURCES...: one object per source, in parallel
compile() {
  cc="$1"
  flags="$2"
  dir="$3"
  shift 3
  mkdir -p "$dir"
  printf '%s\n' "$@" | xargs -P "$(nproc 2>/dev/null || echo 4)" -I{} \
    sh -c "f={}; b=\${f##*/}; exec $cc $flags -c \$f -o $dir/\${b%.*}.o"
}

# MiniSat: the simplifying solver and its options, without System.cc (see main/minisat.cc)
m="$work/minisat"
compile "$CXX" "$target -std=c++98 -O3 -DNDEBUG -D__STDC_LIMIT_MACROS -D__STDC_FORMAT_MACROS -I$here/main -I$m" \
  "$m/obj" "$m/minisat/core/Solver.cc" "$m/minisat/simp/SimpSolver.cc" \
  "$m/minisat/utils/Options.cc" "$here/main/minisat.cc"
# shellcheck disable=SC2086
$CXX $target -o "$work/out/minisat$ext" "$m"/obj/*.o $libs

# CaDiCaL: every source but the model-based tester, as its BUILD.md compiles it without make
c="$work/cadical"
compile "$CXX" "$target -Wall -Wextra -O3 -DNDEBUG -DNBUILD -I$c/src" "$c/obj" \
  $(ls "$c"/src/*.cpp | grep -v -e '/mobical.cpp$' -e '/cadical.cpp$')
compile "$CC" "$target -Wall -Wextra -O3 -DNDEBUG -DNBUILD -I$c/src" "$c/obj" "$c"/src/*.c
compile "$CXX" "$target -Wall -Wextra -O3 -DNDEBUG -DNBUILD -Dmain=cadical_main -I$c/src" \
  "$c/obj" "$c/src/cadical.cpp"
compile "$CXX" "$target -O3 -DNDEBUG" "$c/wrapper" "$here/main/cadical.cpp"
# shellcheck disable=SC2086
$CXX $target -o "$work/out/cadical$ext" "$c"/obj/*.o "$c/wrapper/cadical.o" $libs

# Kissat: the flags of its configure script by default, its build header fixed
k="$work/kissat"
cat >"$k/src/build.h" <<EOF
#define VERSION "$KISSAT"
#define COMPILER "$compiler -O3 -DNDEBUG"
#define ID "unknown"
#define BUILD "dojo/reference/build.sh"
#define DIR "/build"
EOF
compile "$CC" "$target -W -Wall -O3 -DNDEBUG -I$k/src" "$k/obj" \
  $(ls "$k"/src/*.c | grep -v '/main.c$')
compile "$CC" "$target -W -Wall -O3 -DNDEBUG -Dmain=kissat_main -I$k/src" "$k/obj" "$k/src/main.c"
compile "$CC" "$target -O3 -DNDEBUG" "$k/wrapper" "$here/main/kissat.c"
# shellcheck disable=SC2086
$CC $target -o "$work/out/kissat$ext" "$k"/obj/*.o "$k/wrapper/kissat.o" $libs -lm

case $mode in
  native)
    mkdir -p "$out"
    cp "$work/out/minisat" "$work/out/cadical" "$work/out/kissat" "$out/"
    echo "built $out/minisat, $out/cadical and $out/kissat"
    ;;
  wasm)
    cp "$work"/out/*.wasm "$out/"
    (cd "$out" && sha256sum minisat.wasm cadical.wasm kissat.wasm >SHA256SUMS)
    echo "built $out/minisat.wasm, $out/cadical.wasm and $out/kissat.wasm (SHA256SUMS updated)"
    ;;
  check)
    (cd "$work/out" && sha256sum -c "$here/SHA256SUMS")
    for f in minisat cadical kissat; do
      cmp "$work/out/$f.wasm" "$here/$f.wasm"
    done
    echo "the committed reference solvers are what their sources build"
    ;;
esac
