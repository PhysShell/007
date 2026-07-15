"""N2-D1 tokenizer identity: positive/negative conformance fixtures.

Pins qodec's o200k meter (qodec/src/meter.rs -> tiktoken_rs::o200k_base(),
tiktoken-rs 0.7.0 per qodec/Cargo.lock) against committed expected token
counts, independent of any N2 case content. Every expected count here was
derived by actually running the real qodec binary (built ad hoc via
`cargo build --release` for this verification only -- N2-D's canonical build
mechanism remains flake.nix's packages.qodec, per n2d1-contract.json
section_2) over the exact fixture bytes; none are guessed or hand-computed.

Builds the binary once per test session if QODEC_BIN is not already set.
"""
from __future__ import annotations

import json
import os
import subprocess
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[7]
QODEC_DIR = REPO_ROOT / "qodec"

ENCODE_ARGV_TAIL = [
    "encode", "--codec", "fold-grep-guarded", "--meter", "o200k",
    "--passthrough-on-no-gain", "--json",
]

# (fixture bytes, expected tokens_in) -- real counts from the real binary,
# see the module docstring. Not estimates.
FIXTURES = [
    (b"", 0),
    (b"hello world\n", 3),
    (b"hello world\n\n", 3),
    ("日本語テスト".encode("utf-8"), 4),
    (
        b"error: expected one of `)`, `,`, or `->`, found `{`\n"
        b" --> src/main.rs:12:5\n",
        27,
    ),
]

# Deliberately invalid UTF-8 (a lone continuation byte) -- must fail closed,
# per n2d1-contract.json section_1's byte_to_text_decoding_policy.
INVALID_UTF8_FIXTURE = b"\xff\xfe not valid utf-8"


def _resolve_qodec_bin() -> str:
    env_bin = os.environ.get("QODEC_BIN")
    if env_bin and Path(env_bin).is_file():
        return env_bin
    release_bin = QODEC_DIR / "target" / "release" / "qodec"
    if not release_bin.is_file():
        subprocess.run(
            ["cargo", "build", "--release"], cwd=QODEC_DIR, check=True,
            capture_output=True,
        )
    if not release_bin.is_file():
        raise RuntimeError(f"qodec release binary not found at {release_bin} after build")
    return str(release_bin)


class TestTokenizerConformance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.qodec_bin = _resolve_qodec_bin()

    def _encode(self, data: bytes) -> subprocess.CompletedProcess:
        return subprocess.run(
            [self.qodec_bin, *ENCODE_ARGV_TAIL], input=data,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )

    def test_positive_fixtures_match_committed_expected_counts(self):
        for data, expected_tokens in FIXTURES:
            with self.subTest(data=data):
                proc = self._encode(data)
                self.assertEqual(proc.returncode, 0, proc.stderr.decode("utf-8", "replace"))
                envelope = json.loads(proc.stdout.decode("utf-8"))
                self.assertEqual(envelope["meter"], "o200k")
                self.assertEqual(envelope["tokens_in"], expected_tokens)

    def test_invalid_utf8_fails_closed_not_lossy(self):
        """Per the locked byte_to_text_decoding_policy: qodec must error, not
        silently lossy-decode and return a fabricated token count."""
        proc = self._encode(INVALID_UTF8_FIXTURE)
        self.assertNotEqual(proc.returncode, 0)
        self.assertEqual(proc.stdout, b"")

    def test_empty_input_is_zero_tokens_not_an_error(self):
        proc = self._encode(b"")
        self.assertEqual(proc.returncode, 0)
        envelope = json.loads(proc.stdout.decode("utf-8"))
        self.assertEqual(envelope["tokens_in"], 0)

    def test_trailing_blank_line_does_not_change_token_count_for_this_fixture(self):
        """Documents an observed fact (not a general rule): for the
        "hello world" fixture, an extra trailing newline does not add a
        token under o200k_base. This is recorded, not assumed, and must not
        be generalized to other content without its own fixture."""
        one_newline = self._encode(b"hello world\n")
        two_newlines = self._encode(b"hello world\n\n")
        env1 = json.loads(one_newline.stdout.decode("utf-8"))
        env2 = json.loads(two_newlines.stdout.decode("utf-8"))
        self.assertEqual(env1["tokens_in"], env2["tokens_in"])


if __name__ == "__main__":
    unittest.main()
