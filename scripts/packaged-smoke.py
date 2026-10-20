#!/usr/bin/env python3
"""Bounded checks of the production prebuilt images; no Rust/image builds here.

See docs/packaged-compatibility.md for coverage, prerequisites, and limitations.
Only Python's standard library and a local Docker daemon are required.
"""

import argparse
from contextlib import closing
import gzip
import hashlib
import http.client
import io
import json
import os
from pathlib import Path
import re
import shutil
import sqlite3
import subprocess
import tarfile
import tempfile
import time
import uuid


ROOT = Path(__file__).resolve().parent.parent
FIXTURES = ROOT / "scripts/fixtures/packaged"
TOKEN = "packaged-smoke-only-worker-token-00000000"
SESSION = hashlib.sha256(b"packaged-smoke-only-session").hexdigest()


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def write_archive(path):
    """Hand-packed archive, using the documented production `dojo run --set` format."""
    with path.open("wb") as output:
        with gzip.GzipFile(fileobj=output, mode="wb", filename="", mtime=0) as zipped:
            with tarfile.open(fileobj=zipped, mode="w", format=tarfile.USTAR_FORMAT) as archive:
                for source in sorted(FIXTURES.iterdir()):
                    data = source.read_bytes()
                    entry = tarfile.TarInfo("dojo-set/" + source.name)
                    entry.size, entry.mode = len(data), 0o644
                    archive.addfile(entry, io.BytesIO(data))


def assert_verified_checkers(stderr):
    require("drat-trim (" in stderr and "cake_lpr (" in stderr,
            "production proof checkers were not selected")
    require("built-in checker" not in stderr, "built-in checker is forbidden in this gate")


def assert_solver_result(stdout, stderr):
    # `dojo run` exits zero even for unsolved/wrong results. Exit status alone is no gate.
    rows = re.findall(r"\[\s*\d+/2\]\s+(\S+)\s+(SAT|UNSAT)\s+(\S+)", stderr)
    require(sorted(rows) == [("hashi-sat", "SAT", "solved"),
                             ("pigeonhole-unsat", "UNSAT", "solved")],
            f"expected exactly two solved formulas, got {rows}")
    require(re.search(r"^solved 2/2\s+PAR-2", stdout, re.M), "not all formulas solved")
    for answer in ("SAT", "UNSAT"):
        require(re.search(rf"^{answer}\s+1/1\s+PAR-2", stdout, re.M),
                f"missing verified {answer} result")
    assert_verified_checkers(stderr)


def request(url, method="GET", body=None, headers=None):
    # URLs are constructed below from loopback Docker port mappings, never caller URLs.
    host, port = url.removeprefix("http://").split("/", 1)[0].split(":")
    path = "/" + url.removeprefix("http://").split("/", 1)[1]
    connection = http.client.HTTPConnection(host, int(port), timeout=3)
    try:
        connection.request(method, path, body=body, headers=headers or {})
        response = connection.getresponse()
        return response.status, {key.lower(): value for key, value in response.getheaders()}, response.read()
    finally:
        connection.close()


def wait_for(description, check, seconds=30):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(0.1)
    raise RuntimeError(f"deadline exceeded waiting for {description}")


def multipart():
    boundary = "hashi-packaged-smoke-boundary"
    fields = {"family": "packaged-smoke", "description": "Packaged callback regression",
              "claimed": "SAT", "license": "on"}
    parts = [f'--{boundary}\r\nContent-Disposition: form-data; name="{key}"\r\n\r\n{value}\r\n'
             for key, value in fields.items()]
    parts.append(f'--{boundary}\r\nContent-Disposition: form-data; name="cnf"; '
                 'filename="tiny.cnf"\r\nContent-Type: text/plain\r\n\r\n'
                 + (FIXTURES / "hashi-sat.cnf").read_text() + "\r\n")
    parts.append(f"--{boundary}--\r\n")
    return "".join(parts).encode(), "multipart/form-data; boundary=" + boundary


class Smoke:
    def __init__(self, args):
        self.args = args
        self.prefix = "hashi-packaged-" + uuid.uuid4().hex[:12]
        self.containers = []
        self.network = False
        self.paused = None
        self.logs = Path(args.logs).resolve()
        self.logs.mkdir(parents=True, exist_ok=False)

    def docker(self, *args, timeout=30):
        result = subprocess.run([self.args.engine, *args], capture_output=True,
                                text=True, timeout=timeout, check=False)
        require(result.returncode == 0,
                f"container command failed: {args}\n{result.stdout}\n{result.stderr}")
        return result.stdout.strip()

    def run(self, name, image, *options, command=()):
        name = self.prefix + "-" + name
        # Track before calling Docker: cleanup also handles a lost/timed-out response.
        self.containers.append(name)
        self.docker("run", "--pull=never", "--detach", "--name", name, *options, image, *command)
        return name

    def url(self, container):
        address = self.docker("port", container, "8080/tcp")
        require(re.fullmatch(r"127\.0\.0\.1:\d+", address), f"non-loopback port: {address}")
        return "http://" + address

    def collect_logs(self, container):
        # With no TTY Docker forwards stdout/stderr separately, like the original process.
        result = subprocess.run([self.args.engine, "logs", container], capture_output=True,
                                text=True, timeout=10, check=False)
        (self.logs / (container + ".stdout.log")).write_text(result.stdout)
        (self.logs / (container + ".stderr.log")).write_text(result.stderr)
        require(result.returncode == 0, f"could not collect logs for {container}")
        return result

    def ready(self, container, url, path):
        def check():
            running = self.docker("inspect", "--format", "{{.State.Running}}", container)
            require(running == "true", f"{container} exited during startup")
            try:
                return request(url + path)[0] == 200
            except (OSError, http.client.HTTPException):
                return False
        wait_for(container + " readiness", check)

    def solver(self, scratch):
        write_archive(scratch / "tiny.tar.gz")
        shutil.copyfile(ROOT / "dojo/reference/minisat.wasm", scratch / "minisat.wasm")
        container = self.run("solver", self.args.worker_image,
                             "--network", "none", "--user", "10001:10001",
                             "--mount", f"type=bind,src={scratch},dst=/fixtures,readonly",
                             "--entrypoint", "dojo", command=(
                                 "run", "/fixtures/minisat.wasm", "--set", "/fixtures/tiny.tar.gz",
                                 "--checker", "verified", "--threads", "1", "--budget", "10000000"))
        status = self.docker("wait", container, timeout=120)
        require(status == "0", f"packaged solver exited {status}")
        result = self.collect_logs(container)
        assert_solver_result(result.stdout, result.stderr)
        print("ok: packaged MiniSat solved SAT and proved UNSAT with drat-trim + cake_lpr", flush=True)

    def roundtrip(self, scratch):
        data = scratch / "data"
        data.mkdir()
        database = data / "hashi.db"
        common = ("--network", self.prefix, "--publish", "127.0.0.1::8080")
        # Run the site binary as the caller to share the disposable SQLite file with Python.
        # The worker still uses its production entry point and drops to uid 10001 itself.
        site_env = ("--user", f"{os.getuid()}:{os.getgid()}",
                    "--mount", f"type=bind,src={data},dst=/data",
                    "--env", "HASHI_DB_PATH=/data/hashi.db",
                    "--env", "HASHI_DAILY_SECRET=packaged-smoke-only-daily-secret-00000000",
                    "--env", "DOJO_SECRET=packaged-smoke-only-dojo-secret-00000000",
                    "--env", "DOJO_TOKEN=" + TOKEN)
        bootstrap = self.run("schema", self.args.site_image, *common, *site_env)
        self.ready(bootstrap, self.url(bootstrap), "/favicon.svg")
        self.docker("stop", "--time", "5", bootstrap)
        # Same seeded account/session technique as e2e/seed.ts; all work rows use HTTP.
        now = int(time.time() * 1000)
        with closing(sqlite3.connect(database, timeout=3)) as db, db:
            db.execute("INSERT INTO users (id, username, name, created_at) VALUES (1, ?, ?, ?)",
                       ("packaged-smoke", "Packaged Smoke", now - 2 * 86400000))
            db.execute("INSERT INTO sessions (token_hash, user_id, created_at, expires_at) "
                       "VALUES (?, 1, ?, ?)",
                       (hashlib.sha256(SESSION.encode()).hexdigest(), now, now + 86400000))
        with closing(sqlite3.connect(database, timeout=3)) as db:
            db.execute("SELECT COUNT(*) FROM users").fetchone()
            worker = self.run("worker", self.args.worker_image, *common,
                              "--network-alias", "worker", "--env", "DOJO_TOKEN=" + TOKEN,
                              "--env", "APP_URL=http://site:8080", "--env", "DOJO_THREADS=1",
                              "--env", "DOJO_MEMORY_MB=256", "--env", "DOJO_CHECKS=1",
                              "--env", "DOJO_CHECK_SECS=15", "--env", "DOJO_CHECK_MEMORY_MB=512",
                              "--env", "DOJO_PROOF_MAX_MB=4")
            worker_url = self.url(worker)
            self.ready(worker, worker_url, "/health")
            # Valid query/body: auth must refuse it before evaluation or callback.
            status, _, _ = request(worker_url + "/benchmarks?id=999&report_token=invalid",
                                   "POST", (FIXTURES / "hashi-sat.cnf").read_bytes(),
                                   {"Authorization": "Bearer wrong-token"})
            require(status == 401, f"worker accepted bad dispatch auth: {status}")
            site = self.run("site", self.args.site_image, *common, *site_env,
                            "--network-alias", "site", "--env", "DOJO_URL=http://worker:8080")
            site_url = self.url(site)
            self.ready(site, site_url, "/favicon.svg")
            # Hold the real job at the network boundary so checking/token assertions have no race.
            self.docker("pause", worker)
            self.paused = worker
            body, content_type = multipart()
            status, headers, _ = request(site_url + "/dojo/benchmarks/submit", "POST", body,
                                         {"Content-Type": content_type,
                                          "Cookie": "hashi_session=" + SESSION})
            location = headers.get("location", "")
            match = re.fullmatch(r"/dojo/benchmarks\?submitted=(\d+)#mine", location)
            require(status == 303 and match is not None, f"upload failed: {status} {location}")
            benchmark = int(match[1])

            def row():
                return db.execute("SELECT state, reason, job_token, attempts FROM dojo_benchmarks "
                                  "WHERE id = ?", (benchmark,)).fetchone()

            checking = wait_for("authenticated dispatch claim", lambda: (r := row())
                                and r[0] == "checking" and r, seconds=10)
            require(checking[1] is None and checking[3] == 1 and
                    re.fullmatch(r"[a-f0-9]{64}", checking[2] or ""), f"bad claim: {checking}")
            report = json.dumps({"id": benchmark, "verdict": {"Refused": {"reason": "TooEasy"}}})
            for token in ("wrong-token", TOKEN):
                status, _, _ = request(site_url + "/dojo/benchmark-report", "POST", report,
                                       {"Content-Type": "application/json",
                                        "Authorization": "Bearer " + token})
                require(status == 401, f"site accepted non-job callback token: {status}")
                require(row() == checking, "unauthorized callback changed the claimed job")
            self.docker("unpause", worker)
            self.paused = None
            terminal = wait_for("worker callback persisted", lambda: (r := row())
                                and r[0] not in ("queued", "checking") and r, seconds=45)
            require(terminal == ("refused", "too_easy", None, 1), f"unexpected result: {terminal}")
            # Completion retires the real token; a replay must fail without changing the row.
            status, _, _ = request(site_url + "/dojo/benchmark-report", "POST", report,
                                   {"Content-Type": "application/json",
                                    "Authorization": "Bearer " + checking[2]})
            require(status == 401 and row() == terminal, "completed job accepted a replay")
            self.docker("stop", "--time", "5", site, worker)
            assert_verified_checkers(self.collect_logs(worker).stderr)
        (self.logs / "result.json").write_text(json.dumps({
            "benchmark_id": benchmark, "state": terminal[0], "reason": terminal[1],
            "attempts": terminal[3], "token_retired": terminal[2] is None,
        }, indent=2) + "\n")
        print("ok: HTTP upload -> authenticated dispatch -> worker -> authenticated callback -> DB; replay refused",
              flush=True)

    def cleanup(self):
        failures = []
        commands = []
        if self.paused:
            commands.append(("unpause", self.paused))
        for name in reversed(self.containers):
            try:
                result = self.collect_logs(name)
                require(" panicked at " not in result.stderr, f"panic in {name}")
            except Exception as error:
                failures.append(str(error))
            commands.append(("rm", "--force", name))
        if self.network:
            commands.append(("network", "rm", self.prefix))
        for command in commands:
            try:
                self.docker(*command, timeout=15)
            except Exception as error:
                failures.append(str(error))
        require(not failures, "cleanup/log errors: " + "; ".join(failures))

    def execute(self):
        # Fail immediately on absent daemon/images; never skip, pull, build, or use host binaries.
        self.docker("info", "--format", "{{.ServerVersion}}")
        for image in (self.args.site_image, self.args.worker_image):
            self.docker("image", "inspect", image)
        with tempfile.TemporaryDirectory(prefix=self.prefix + "-") as directory:
            scratch = Path(directory)
            scratch.chmod(0o755)
            try:
                self.network = True
                self.docker("network", "create", "--driver", "bridge", self.prefix)
                self.solver(scratch)
                self.roundtrip(scratch)
            finally:
                self.cleanup()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--site-image", required=True)
    parser.add_argument("--worker-image", required=True)
    parser.add_argument("--logs", required=True, help="new directory; logs survive failure")
    parser.add_argument("--engine", default="docker", help="container executable (one argument)")
    Smoke(parser.parse_args()).execute()


if __name__ == "__main__":
    main()
