"""Fast harness regression tests. These do not stand in for the packaged image job."""

import importlib.util
import itertools
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).resolve().parents[1] / "packaged-smoke.py"
SPEC = importlib.util.spec_from_file_location("packaged_smoke", SCRIPT)
smoke = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(smoke)

OUT = "solved 2/2  PAR-2 0.001 Gfuel\nSAT    1/1  PAR-2 0.001\nUNSAT  1/1  PAR-2 0.001\n"
ERR = ("proofs are checked with drat-trim (/usr/local/bin/drat-trim) and "
       "cake_lpr (/usr/local/bin/cake_lpr), like the leaderboard\n"
       "[ 1/2] hashi-sat        SAT   solved       0.001 Gfuel     2 ms\n"
       "[ 2/2] pigeonhole-unsat UNSAT solved       0.001 Gfuel     2 ms\n")


class PackagedSmokeTests(unittest.TestCase):
    def test_two_solved_results_with_verified_checkers_are_accepted(self):
        smoke.assert_solver_result(OUT, ERR)

    def test_zero_exit_unsolved_cli_output_fails(self):
        # dojo run returns success even on these outcomes; this was the important false green.
        for outcome in ("timeout", "no proof", "WRONG", "crashed", "unknown"):
            with self.subTest(outcome=outcome), self.assertRaises(RuntimeError):
                smoke.assert_solver_result(OUT, ERR.replace("UNSAT solved", "UNSAT " + outcome))

    def test_builtin_or_absent_external_checkers_fail(self):
        for error in (ERR.replace("drat-trim (", "missing ("),
                      ERR.replace("cake_lpr (", "missing ("), ERR + "built-in checker"):
            with self.subTest(error=error), self.assertRaises(RuntimeError):
                smoke.assert_solver_result(OUT, error)

    def test_wrong_missing_duplicate_or_extra_formula_fails(self):
        for error in (ERR.replace("hashi-sat", "hashi-other"),
                      ERR.replace("UNSAT solved", "SAT solved"),
                      ERR.split("[ 2/2]")[0],
                      ERR + "[ 2/2] pigeonhole-unsat UNSAT solved\n"):
            with self.subTest(error=error), self.assertRaises(RuntimeError):
                smoke.assert_solver_result(OUT, error)

    def test_summary_requires_both_sat_and_unsat_success(self):
        for output in (OUT.replace("solved 2/2", "solved 1/2"),
                       OUT.replace("SAT    1/1", "SAT    0/1"),
                       OUT.replace("UNSAT  1/1", "UNSAT  0/1")):
            with self.subTest(output=output), self.assertRaises(RuntimeError):
                smoke.assert_solver_result(output, ERR)

    def test_fixtures_are_deterministic_and_independently_correct(self):
        with tempfile.TemporaryDirectory() as directory:
            first, second = Path(directory) / "a.tar.gz", Path(directory) / "b.tar.gz"
            smoke.write_archive(first)
            smoke.write_archive(second)
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with tarfile.open(first) as archive:
                answers = archive.extractfile("dojo-set/answers.txt").read().decode().splitlines()
                self.assertEqual(answers, ["hashi-sat SAT", "pigeonhole-unsat UNSAT"])
                for answer in answers:
                    name, expected = answer.split()
                    lines = archive.extractfile("dojo-set/" + name + ".cnf").read().decode().splitlines()
                    header = next(line.split() for line in lines if line.startswith("p cnf"))
                    clauses = [[int(x) for x in line.split()[:-1]] for line in lines
                               if line and line[0] not in ("c", "p")]
                    self.assertEqual(len(clauses), int(header[3]))
                    # An independent exhaustive oracle over at most six variables.
                    def satisfies(model):
                        return all(any(model[abs(lit) - 1] == (lit > 0) for lit in clause)
                                   for clause in clauses)
                    sat = any(satisfies(model) for model in
                              itertools.product((False, True), repeat=int(header[2])))
                    self.assertEqual(sat, expected == "SAT", name)
                    if sat:
                        witness = archive.extractfile("dojo-set/witnesses.txt").read().decode().split()
                        self.assertEqual(witness[0], name)
                        model = [int(lit) > 0 for lit in witness[1:-1]]
                        self.assertTrue(satisfies(model))

    def test_polling_has_a_deadline(self):
        with patch.object(smoke.time, "monotonic", side_effect=[0, 0, 1]), \
                patch.object(smoke.time, "sleep") as sleep:
            with self.assertRaisesRegex(RuntimeError, "deadline exceeded"):
                smoke.wait_for("callback", lambda: False, seconds=1)
            sleep.assert_called_once_with(0.1)

    def test_claim_survives_a_missed_immediate_dispatch(self):
        # The startup dispatcher may own BUSY when submit tries to dispatch.
        # The next production tick is 30 seconds later, beyond the old 10s limit.
        clock = [0.0]
        claimed = ("checking", None, "a" * 64, 1)
        def read_row():
            return claimed if clock[0] >= 31 else ("queued", None, None, 0)
        def sleep(seconds):
            clock[0] += seconds
        with patch.object(smoke.time, "monotonic", side_effect=lambda: clock[0]), \
                patch.object(smoke.time, "sleep", side_effect=sleep):
            self.assertEqual(smoke.wait_for_dispatch_claim(read_row), claimed)
        self.assertGreaterEqual(clock[0], 31)

    def test_claim_never_dispatched_still_fails_within_a_bound(self):
        clock = [0.0]
        def sleep(seconds):
            clock[0] += seconds
        with patch.object(smoke.time, "monotonic", side_effect=lambda: clock[0]), \
                patch.object(smoke.time, "sleep", side_effect=sleep):
            with self.assertRaisesRegex(RuntimeError, "last state/reason/attempts:.*queued"):
                smoke.wait_for_dispatch_claim(lambda: ("queued", None, None, 0))
        self.assertLess(clock[0], 46)

    def test_cleanup_continues_after_a_container_removal_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            args = type("Args", (), {"logs": str(Path(directory) / "logs"), "engine": "docker"})()
            run = smoke.Smoke(args)
            run.containers = ["solver", "site", "worker"]
            run.network, run.paused = True, "worker"
            completed = type("Completed", (), {"stdout": "", "stderr": "", "returncode": 0})()
            def remove(*command, **_kwargs):
                if command == ("rm", "--force", "worker"):
                    raise RuntimeError("removal failed")
            with patch.object(smoke.subprocess, "run", return_value=completed), \
                    patch.object(run, "docker", side_effect=remove) as docker:
                with self.assertRaisesRegex(RuntimeError, "removal failed"):
                    run.cleanup()
                self.assertEqual([call.args for call in docker.call_args_list], [
                    ("unpause", "worker"), ("rm", "--force", "worker"),
                    ("rm", "--force", "site"), ("rm", "--force", "solver"),
                    ("network", "rm", run.prefix),
                ])
            self.assertEqual(len(list(run.logs.glob("*.log"))), 6)


if __name__ == "__main__":
    unittest.main()
