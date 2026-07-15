"""Scope N2-A.1 regression coverage (addendum section 6): the sanitizer must
stay order-preserving. Two compact synthetic fixtures reproduce the real
observed warning-ordering difference from workflow run 29371996936 — the
Action/EncryptData.cs SYSLIB0021 warnings appearing before, or after, the
Program.cs CS8604/CS8602 warnings. Responsibility for eliminating that
difference belongs to the deterministic build argv (--disable-build-servers
in source-manifest.json), never to the sanitizer.
"""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import sanitizer  # noqa: E402

# The real diagnostic lines named in the N2-A.1 addendum (section 2),
# reproduced verbatim modulo a synthetic temp-path prefix.
_SYSLIB_1 = (
    "        <TMP>/EncryptAesApp/Action/EncryptData.cs(12,20): warning SYSLIB0021: "
    "'AesManaged' is obsolete: 'Derived cryptographic types are obsolete. Use the "
    "Create method on the base type instead.' (https://aka.ms/dotnet-warnings/SYSLIB0021) "
    "[<TMP>/EncryptAesApp/EncryptAesApp.csproj]"
)
_SYSLIB_2 = (
    "        <TMP>/EncryptAesApp/Action/EncryptData.cs(12,41): warning SYSLIB0021: "
    "'AesManaged' is obsolete: 'Derived cryptographic types are obsolete. Use the "
    "Create method on the base type instead.' (https://aka.ms/dotnet-warnings/SYSLIB0021) "
    "[<TMP>/EncryptAesApp/EncryptAesApp.csproj]"
)
_CS8604 = (
    "        <TMP>/EncryptAesApp/Program.cs(12,35): warning CS8604: Possible null "
    "reference argument for parameter 'raw' in 'void EncryptData.EncryptAesManaged"
    "(string raw)'. [<TMP>/EncryptAesApp/EncryptAesApp.csproj]"
)
_CS8602 = (
    "        <TMP>/EncryptAesApp/Program.cs(16,20): warning CS8602: Dereference of "
    "a possibly null reference. [<TMP>/EncryptAesApp/EncryptAesApp.csproj]"
)


def _build_order_a(tmp_root: str) -> bytes:
    # Observed order: SYSLIB0021 warnings first, then the CS8604/CS8602 pair —
    # matches capture-a's actual real ordering.
    lines = [_SYSLIB_1, _SYSLIB_2, _CS8604, _CS8602]
    text = "\n".join(lines).replace("<TMP>", tmp_root)
    return text.encode("utf-8")


def _build_order_b(tmp_root: str) -> bytes:
    # Observed order: CS8604/CS8602 pair first, then the SYSLIB0021 warnings —
    # matches capture-b's actual real ordering.
    lines = [_CS8604, _CS8602, _SYSLIB_1, _SYSLIB_2]
    text = "\n".join(lines).replace("<TMP>", tmp_root)
    return text.encode("utf-8")


class TestSanitizerStaysOrderPreserving(unittest.TestCase):
    def test_two_real_observed_orders_remain_different_after_sanitization(self):
        tmp_root = "/home/runner/work/_temp/work-a"
        raw_a = _build_order_a(tmp_root)
        raw_b = _build_order_b(tmp_root)
        sanitized_a, _ = sanitizer.sanitize(raw_a, tmp_root=tmp_root)
        sanitized_b, _ = sanitizer.sanitize(raw_b, tmp_root=tmp_root)
        self.assertNotEqual(
            sanitizer.sha256_bytes(sanitized_a), sanitizer.sha256_bytes(sanitized_b),
            "the sanitizer must never manufacture equality between two differently-"
            "ordered diagnostic streams — that responsibility belongs to the build argv",
        )

    def test_sanitizer_does_not_reorder_lines_within_each_input(self):
        tmp_root = "/home/runner/work/_temp/work-a"
        raw_a = _build_order_a(tmp_root)
        sanitized_a, _ = sanitizer.sanitize(raw_a, tmp_root=tmp_root)
        text = sanitized_a.decode("utf-8")
        syslib_first_line_idx = text.index("SYSLIB0021")
        cs8604_idx = text.index("CS8604")
        self.assertLess(
            syslib_first_line_idx, cs8604_idx,
            "sanitizing order-a must preserve SYSLIB0021 appearing before CS8604, "
            "exactly as in the real capture-a ordering",
        )

    def test_sanitizer_does_not_reorder_lines_within_the_reversed_input(self):
        tmp_root = "/home/runner/work/_temp/work-b"
        raw_b = _build_order_b(tmp_root)
        sanitized_b, _ = sanitizer.sanitize(raw_b, tmp_root=tmp_root)
        text = sanitized_b.decode("utf-8")
        cs8604_idx = text.index("CS8604")
        syslib_first_line_idx = text.index("SYSLIB0021")
        self.assertLess(
            cs8604_idx, syslib_first_line_idx,
            "sanitizing order-b must preserve CS8604 appearing before SYSLIB0021, "
            "exactly as in the real capture-b ordering",
        )

    def test_sanitizer_never_deduplicates_the_two_identical_syslib_warnings(self):
        tmp_root = "/home/runner/work/_temp/work-a"
        raw_a = _build_order_a(tmp_root)
        sanitized_a, _ = sanitizer.sanitize(raw_a, tmp_root=tmp_root)
        self.assertEqual(sanitized_a.decode("utf-8").count("warning SYSLIB0021:"), 2)

    def test_semantic_content_identical_only_ordering_differs(self):
        # Same four diagnostic lines, same tmp_root; only the arrangement
        # differs. Sorting both sanitized outputs' lines must produce
        # byte-identical results — the *content* multiset is unaffected by
        # order, proving the earlier hash difference is ordering-only.
        tmp_root = "/home/runner/work/_temp/work-shared"
        sanitized_a, _ = sanitizer.sanitize(_build_order_a(tmp_root), tmp_root=tmp_root)
        sanitized_b, _ = sanitizer.sanitize(_build_order_b(tmp_root), tmp_root=tmp_root)
        lines_a = sorted(sanitized_a.decode("utf-8").splitlines())
        lines_b = sorted(sanitized_b.decode("utf-8").splitlines())
        self.assertEqual(lines_a, lines_b)


if __name__ == "__main__":
    unittest.main()
