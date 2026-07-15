"""Unit tests for network_enforcement_probe.py -- the jvm-gradle
network_enforcement_mode = "outer-netns-loopback-only" live-verification
gate. Mocks capture_build.run_real_build so these tests never require a
real kernel Landlock/netns environment (only real CI runners have that);
they test the aggregation/decision logic against canned, realistic probe
output shapes.
"""
import json
import sys
import unittest
import unittest.mock as mock
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import network_enforcement_probe as nep  # noqa: E402


def _fake_run_real_build_factory(negative_stdout: bytes, negative_exit: int,
                                  positive_stdout: bytes, positive_exit: int):
    def _fake(sandboy_bin, policy_path, cwd, argv, env):
        script = argv[-1]
        if "network_probe.py" in script:
            return {"argv": argv, "exit_code": negative_exit, "raw_stdout": negative_stdout, "raw_stderr": b""}
        return {"argv": argv, "exit_code": positive_exit, "raw_stdout": positive_stdout, "raw_stderr": b""}
    return _fake


REAL_NEGATIVE_ALL_BLOCKED = json.dumps({
    "probe_results": [
        {"host": "1.1.1.1", "port": 443, "kind": "tcp", "result": "UNREACHABLE", "error": "OSError: [Errno 101] Network is unreachable"},
        {"host": "8.8.8.8", "port": 53, "kind": "udp", "result": "UNREACHABLE", "error": "OSError: [Errno 101] Network is unreachable"},
        {"host": "api.nuget.org", "port": 443, "kind": "tcp-dns-plus-connect", "result": "UNREACHABLE", "error": "socket.gaierror: [Errno -3] Temporary failure in name resolution"},
    ]
}).encode()

REAL_NEGATIVE_ONE_REACHABLE = json.dumps({
    "probe_results": [
        {"host": "1.1.1.1", "port": 443, "kind": "tcp", "result": "REACHABLE"},
    ]
}).encode()

REAL_POSITIVE_ALLOWED = json.dumps({"result": "ALLOWED", "bound_port": 54321, "echoed_correctly": True}).encode()
REAL_POSITIVE_DENIED = json.dumps({"result": "DENIED", "errno": 13, "error": "[Errno 13] Permission denied"}).encode()


class TestRunGradleNetworkEnforcementChecks(unittest.TestCase):
    def _run(self, negative_stdout, negative_exit, positive_stdout, positive_exit):
        fake = _fake_run_real_build_factory(negative_stdout, negative_exit, positive_stdout, positive_exit)
        with mock.patch.object(nep.capture_build, "run_real_build", side_effect=fake):
            return nep.run_gradle_network_enforcement_checks(
                sandboy_bin=Path("/nonexistent/sandboy"), policy_path=Path("/nonexistent/policy.toml"),
                cwd=Path("/nonexistent/cwd"), env={},
            )

    def test_external_blocked_and_loopback_allowed_verifies_the_exception(self):
        report = self._run(REAL_NEGATIVE_ALL_BLOCKED, 0, REAL_POSITIVE_ALLOWED, 0)
        self.assertTrue(report["external_connectivity_confirmed_blocked"])
        self.assertTrue(report["loopback_bind_connect_confirmed_allowed"])
        self.assertTrue(report["enforcement_exception_verified"])

    def test_a_reachable_external_target_fails_verification_even_if_loopback_works(self):
        report = self._run(REAL_NEGATIVE_ONE_REACHABLE, 1, REAL_POSITIVE_ALLOWED, 0)
        self.assertFalse(report["external_connectivity_confirmed_blocked"])
        self.assertFalse(report["enforcement_exception_verified"])

    def test_a_denied_loopback_bind_fails_verification_even_if_external_is_blocked(self):
        report = self._run(REAL_NEGATIVE_ALL_BLOCKED, 0, REAL_POSITIVE_DENIED, 1)
        self.assertFalse(report["loopback_bind_connect_confirmed_allowed"])
        self.assertFalse(report["enforcement_exception_verified"])

    def test_report_records_both_raw_probe_reports_with_hashes(self):
        report = self._run(REAL_NEGATIVE_ALL_BLOCKED, 0, REAL_POSITIVE_ALLOWED, 0)
        import hashlib
        self.assertEqual(
            report["negative_external_connectivity_probe"]["raw_stdout_sha256"],
            hashlib.sha256(REAL_NEGATIVE_ALL_BLOCKED).hexdigest(),
        )
        self.assertEqual(
            report["positive_loopback_bind_connect_probe"]["raw_stdout_sha256"],
            hashlib.sha256(REAL_POSITIVE_ALLOWED).hexdigest(),
        )

    def test_report_type_and_mode_fields_present(self):
        report = self._run(REAL_NEGATIVE_ALL_BLOCKED, 0, REAL_POSITIVE_ALLOWED, 0)
        self.assertEqual(report["report_type"], "n2d1b-gradle-network-enforcement-probe-v1")
        self.assertEqual(report["network_enforcement_mode"], "outer-netns-loopback-only")


if __name__ == "__main__":
    unittest.main()
