import importlib.util
import subprocess
import sys
import time
import unittest
from pathlib import Path


def verifier_module():
    path = Path(__file__).with_name("verify-release-archive.py")
    spec = importlib.util.spec_from_file_location("verify_release_archive", path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class ReleaseArchiveVerifierTests(unittest.TestCase):
    def test_run_returns_after_parent_exits_when_descendant_keeps_capture_handles(self):
        module = verifier_module()
        command = [
            sys.executable,
            "-c",
            "import subprocess, sys; subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(1)']); print('parent')",
        ]

        started = time.monotonic()
        result = module.run(command, dict(), timeout_seconds=0.1)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout.strip(), "parent")
        self.assertLess(time.monotonic() - started, 0.5)


if __name__ == "__main__":
    unittest.main()
