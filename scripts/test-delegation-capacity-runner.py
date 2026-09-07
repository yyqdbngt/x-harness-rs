#!/usr/bin/env python3
"""Exercise evidence collection without compiling or executing Rust."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "capacity_runner", Path(__file__).with_name("run-delegation-capacity-experiment.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class EvidenceTests(unittest.TestCase):
    def sample(self, body):
        with tempfile.TemporaryDirectory(prefix="xharness-capacity-collector-test-") as root:
            script = Path(root) / "fixture.py"
            script.write_text("import time\ntime.sleep(0.08)\n" + body, encoding="utf-8")
            output = io.StringIO()
            with contextlib.redirect_stdout(output):
                runner.sample(sys.executable, str(script), str(Path(root) / "fixture.log"))
            return json.loads(output.getvalue())

    def test_collects_real_process_metrics(self):
        row = self.sample('print(\'CAPACITY_RESULT {"kind":"fixture","capacity":4}\')\n')
        self.assertEqual(row["capacity"], 4)
        self.assertGreater(row["peak_rss_bytes"], 0)
        self.assertGreaterEqual(row["cpu_seconds"], 0)
        self.assertGreater(row["process_wall_ms"], 0)

    def test_missing_result_is_not_success(self):
        with self.assertRaisesRegex(RuntimeError, "exactly one measurement"):
            self.sample('print("no test matched")\n')

    def test_failed_test_cannot_publish_success(self):
        with self.assertRaisesRegex(RuntimeError, "test exited 7"):
            self.sample('print(\'CAPACITY_RESULT {"kind":"fixture"}\')\nraise SystemExit(7)\n')

    def test_duplicate_results_are_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "exactly one measurement"):
            self.sample('print(\'CAPACITY_RESULT {}\\nCAPACITY_RESULT {}\')\n')


if __name__ == "__main__":
    unittest.main()
