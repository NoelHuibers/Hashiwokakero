"""Cold-burst diagnostic for docs/performance/generation-coalescing.md: starts a built hashi
server on a fresh database, fires N concurrent start_daily requests at it (no daily stored yet,
so every request needs today's puzzle) and prints wall time, statuses, distinct puzzles and the
server process's CPU time as JSON lines. With a path it GETs that instead (an OG image).

usage: python3 -I examples/generation_burst.py <binary> <site root> <port> <requests> <runs>
           <scratch dir> [GET path]

<binary> and <site root> are target/debug/hashi and target/site after `cargo leptos build`
(hash.txt must lie next to the binary). Each run restarts the server on a new database file in
<scratch dir> and deletes it afterwards. Rate limits are switched off (dev only).
"""

import json
import os
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request

binary, site, port, n, runs, scratch = sys.argv[1:7]
path = sys.argv[7] if len(sys.argv) > 7 else None
port, n, runs = int(port), int(n), int(runs)
tick = os.sysconf("SC_CLK_TCK")


def cpu_s(pid):
    with open(f"/proc/{pid}/stat") as f:
        fields = f.read().rsplit(")", 1)[1].split()
    return (int(fields[11]) + int(fields[12])) / tick


def wait_up():
    for _ in range(600):
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{port}/robots.txt", timeout=1)
            return
        except Exception:
            time.sleep(0.1)
    raise SystemExit("server did not start")


def start_daily(results, i):
    if path:
        req = urllib.request.Request(f"http://127.0.0.1:{port}{path}")
    else:
        req = urllib.request.Request(
            f"http://127.0.0.1:{port}/api/start_daily",
            data=b'{"device":null}',
            headers={"content-type": "application/json"},
            method="POST",
        )
    t0 = time.monotonic()
    try:
        with urllib.request.urlopen(req, timeout=120) as r:
            raw = r.read()
            status = r.status
            puzzle = str(hash(raw)) if path else json.dumps(json.loads(raw).get("puzzle"), sort_keys=True)
    except urllib.error.HTTPError as e:
        status, puzzle = e.code, e.read()[:80].decode(errors="replace")
    results[i] = (status, time.monotonic() - t0, puzzle)


out = []
for run in range(runs):
    db = os.path.join(scratch, f"burst-{os.getpid()}-{run}.db")
    for suffix in ("", "-wal", "-shm"):
        if os.path.exists(db + suffix):
            os.remove(db + suffix)
    env = dict(
        os.environ,
        LEPTOS_HASH_FILES="true",
        LEPTOS_SITE_ADDR=f"127.0.0.1:{port}",
        LEPTOS_SITE_ROOT=site,
        LEPTOS_OUTPUT_NAME="hashi",
        LEPTOS_SITE_PKG_DIR="pkg",
        LEPTOS_ENV="dev",
        HASHI_RATE_LIMITS="off",
        HASHI_DB_PATH=db,
    )
    server = subprocess.Popen(
        [binary], env=env, cwd=os.path.dirname(binary),
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    try:
        wait_up()
        time.sleep(0.5)
        before = cpu_s(server.pid)
        results = [None] * n
        threads = [threading.Thread(target=start_daily, args=(results, i)) for i in range(n)]
        t0 = time.monotonic()
        for t in threads:
            t.start()
        for t in threads:
            t.join()
        wall = time.monotonic() - t0
        cpu = cpu_s(server.pid) - before
    finally:
        server.terminate()
        server.wait()
    statuses = {}
    for status, _, _ in results:
        statuses[status] = statuses.get(status, 0) + 1
    ok = [r for r in results if r[0] == 200]
    latencies = sorted(r[1] for r in results)
    row = {
        "run": run,
        "wall_s": round(wall, 3),
        "server_cpu_s": round(cpu, 2),
        "statuses": statuses,
        "distinct_puzzles": len({r[2] for r in ok}),
        "p50_s": round(latencies[len(latencies) // 2], 3),
        "max_s": round(latencies[-1], 3),
        "errors": sorted({r[2] for r in results if r[0] != 200}),
    }
    out.append(row)
    print(json.dumps(row), flush=True)
    for suffix in ("", "-wal", "-shm"):
        if os.path.exists(db + suffix):
            os.remove(db + suffix)
