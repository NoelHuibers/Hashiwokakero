#!/bin/sh
# Repeats the calibration study of dojo/CALIBRATION.md: builds the seven solvers for WebAssembly
# and natively, exports the formulas, measures fuel and native time and writes the report.
#
#   dojo/calibration/run.sh OUT [CPU]
#
# OUT gets everything (about 1 GB with the solvers' builds); fuel.csv, time.csv,
# conditions.log and report.md are what dojo/calibration/ holds. CPU is the core the timing
# runs pin to (default 3; take a performance core on a hybrid CPU). The timing step runs one
# thing at a time and takes about an hour: run it on a quiet machine. Needs cargo with the
# wasm32-wasip1 target, a C and C++ compiler for the native reference solvers (or set
# NATIVE_IN_PODMAN=1 to build them in the rust:1.99-trixie image), curl and coreutils.
set -eu

out="${1:?usage: run.sh OUT [CPU]}"
cpu="${2:-3}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
mkdir -p "$out/wasm" "$out/native" "$out/sets" "$out/scratch"
out="$(cd "$out" && pwd)"
cd "$root"

# The solvers
cargo build --locked --release -p dojo --features runner --bin dojo
dojo="$root/target/release/dojo"
cargo build --locked --release -p house --target wasm32-wasip1
cargo build --locked --release -p house
for s in dpll cdcl splr; do
  cp "target/wasm32-wasip1/release/$s.wasm" "$out/wasm/"
  cp "target/release/$s" "$out/native/"
done
cargo build --locked --release --manifest-path dojo/starter/Cargo.toml --target wasm32-wasip1
cargo build --locked --release --manifest-path dojo/starter/Cargo.toml
cp dojo/starter/target/wasm32-wasip1/release/dojo-starter.wasm "$out/wasm/starter.wasm"
cp dojo/starter/target/release/dojo-starter "$out/native/starter"
cp dojo/reference/*.wasm "$out/wasm/"
if [ "${NATIVE_IN_PODMAN:-}" = 1 ]; then
  podman run --rm -v "$root:/repo:ro,Z" -v "$out/native:/out:Z" docker.io/library/rust:1.99-trixie \
    /repo/dojo/reference/build.sh --native /out
else
  dojo/reference/build.sh --native "$out/native"
fi

# The formulas: the dev set and three hidden sets, one per held-out family
"$dojo" export "$out/sets/dev.tar.gz"
for i in 1 2 3; do
  "$dojo" export "$out/sets/hidden-$i.tar.gz" --hidden --seed "calibration-$i"
done
sets="--set $out/sets/dev.tar.gz --set $out/sets/hidden-1.tar.gz --set $out/sets/hidden-2.tar.gz --set $out/sets/hidden-3.tar.gz"
solvers="minisat cadical kissat splr cdcl dpll starter"

# Fuel: deterministic, in parallel
args=""
for s in $solvers; do
  args="$args --solver $s=$out/wasm/$s.wasm"
done
# shellcheck disable=SC2086
"$dojo" calibrate fuel $args $sets --checker verified --out "$out/fuel.csv"

# Time: one solver after another, pinned, the conditions noted
: >"$out/conditions.log"
for s in $solvers; do
  {
    echo "== $s start $(date -Is)"
    echo "uptime: $(uptime)"
    echo "cpu$cpu: $(cat /sys/devices/system/cpu/cpu$cpu/cpufreq/scaling_cur_freq 2>/dev/null) kHz"
  } >>"$out/conditions.log"
  # shellcheck disable=SC2086
  taskset -c "$cpu" "$dojo" calibrate time --solver "$s=$out/wasm/$s.wasm:$out/native/$s" $sets \
    --fuel "$out/fuel.csv" --runs 3 --timeout 5 --scratch "$out/scratch" --out "$out/time-$s.csv"
  echo "== $s end $(date -Is), uptime: $(uptime)" >>"$out/conditions.log"
done
head -n 1 "$out/time-minisat.csv" >"$out/time.csv"
for s in $solvers; do
  tail -n +2 "$out/time-$s.csv" >>"$out/time.csv"
done

"$dojo" calibrate analyze "$out/fuel.csv" "$out/time.csv" >"$out/report.md"
echo "written to $out/report.md"
