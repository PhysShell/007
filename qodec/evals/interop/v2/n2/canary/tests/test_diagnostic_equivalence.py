"""Unit tests for diagnostic_equivalence.py's warning-multiset parsing and
comparison (addendum section 5). Uses the real observed warning lines from
the N2-A.1 finding."""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
sys.path.insert(0, str(TOOLS))
import diagnostic_equivalence as de  # noqa: E402

SOURCE_ROOT = Path("/tmp/fake-source")

_RAW_STDOUT_TEMPLATE = """Build started.
     1>{root}/EncryptAesApp/Program.cs(12,35): warning CS8604: Possible null reference argument for parameter 'raw' in 'void EncryptData.EncryptAesManaged(string raw)'. [{root}/EncryptAesApp/EncryptAesApp.csproj]
     1>{root}/EncryptAesApp/Program.cs(16,20): warning CS8602: Dereference of a possibly null reference. [{root}/EncryptAesApp/EncryptAesApp.csproj]
     1>{root}/EncryptAesApp/Action/EncryptData.cs(12,20): warning SYSLIB0021: 'AesManaged' is obsolete: 'Derived cryptographic types are obsolete. Use the Create method on the base type instead.' (https://aka.ms/dotnet-warnings/SYSLIB0021) [{root}/EncryptAesApp/EncryptAesApp.csproj]
Build succeeded.
    3 Warning(s)
    0 Error(s)
"""


class TestParseWarningMultiset(unittest.TestCase):
    def test_parses_all_three_warnings(self):
        text = _RAW_STDOUT_TEMPLATE.format(root=str(SOURCE_ROOT))
        multiset = de.parse_warning_multiset(text, SOURCE_ROOT)
        self.assertEqual(len(multiset), 3)

    def test_relativizes_file_and_project_paths(self):
        text = _RAW_STDOUT_TEMPLATE.format(root=str(SOURCE_ROOT))
        multiset = de.parse_warning_multiset(text, SOURCE_ROOT)
        files = {key[0] for key in multiset}
        self.assertIn("EncryptAesApp/Program.cs", files)
        self.assertIn("EncryptAesApp/Action/EncryptData.cs", files)

    def test_extracts_line_column_code(self):
        text = _RAW_STDOUT_TEMPLATE.format(root=str(SOURCE_ROOT))
        multiset = de.parse_warning_multiset(text, SOURCE_ROOT)
        cs8604_keys = [k for k in multiset if k[3] == "CS8604"]
        self.assertEqual(len(cs8604_keys), 1)
        _, line, col, code, message, project = cs8604_keys[0]
        self.assertEqual(line, 12)
        self.assertEqual(col, 35)
        self.assertIn("Possible null reference argument", message)

    def test_order_of_lines_does_not_affect_parsed_multiset(self):
        root = str(SOURCE_ROOT)
        lines = _RAW_STDOUT_TEMPLATE.format(root=root).splitlines()
        warning_lines = [line for line in lines if ": warning " in line]
        reversed_text = "\n".join(reversed(warning_lines))
        forward_text = "\n".join(warning_lines)
        self.assertEqual(
            de.parse_warning_multiset(forward_text, SOURCE_ROOT),
            de.parse_warning_multiset(reversed_text, SOURCE_ROOT),
        )


class TestCompareMultisets(unittest.TestCase):
    def test_identical_multisets_are_equivalent(self):
        text = _RAW_STDOUT_TEMPLATE.format(root=str(SOURCE_ROOT))
        pre = de.parse_warning_multiset(text, SOURCE_ROOT)
        post = de.parse_warning_multiset(text, SOURCE_ROOT)
        result = de.compare_multisets(pre, post)
        self.assertTrue(result["equivalent"])
        self.assertEqual(result["added"], [])
        self.assertEqual(result["removed"], [])
        self.assertEqual(result["changed"], [])

    def test_missing_warning_is_reported_as_removed(self):
        text = _RAW_STDOUT_TEMPLATE.format(root=str(SOURCE_ROOT))
        pre = de.parse_warning_multiset(text, SOURCE_ROOT)
        post_text = "\n".join(
            line for line in text.splitlines() if "CS8602" not in line
        )
        post = de.parse_warning_multiset(post_text, SOURCE_ROOT)
        result = de.compare_multisets(pre, post)
        self.assertFalse(result["equivalent"])
        self.assertEqual(len(result["removed"]), 1)
        self.assertEqual(result["removed"][0]["code"], "CS8602")

    def test_extra_warning_is_reported_as_added(self):
        text = _RAW_STDOUT_TEMPLATE.format(root=str(SOURCE_ROOT))
        pre_text = "\n".join(line for line in text.splitlines() if "CS8602" not in line)
        pre = de.parse_warning_multiset(pre_text, SOURCE_ROOT)
        post = de.parse_warning_multiset(text, SOURCE_ROOT)
        result = de.compare_multisets(pre, post)
        self.assertFalse(result["equivalent"])
        self.assertEqual(len(result["added"]), 1)
        self.assertEqual(result["added"][0]["code"], "CS8602")

    def test_duplicated_occurrence_count_change_is_reported(self):
        text = _RAW_STDOUT_TEMPLATE.format(root=str(SOURCE_ROOT))
        pre = de.parse_warning_multiset(text, SOURCE_ROOT)
        # Duplicate the whole stdout (as the real live+summary double-print
        # does) so every warning's occurrence count doubles.
        post = de.parse_warning_multiset(text + text, SOURCE_ROOT)
        result = de.compare_multisets(pre, post)
        self.assertFalse(result["equivalent"])
        self.assertEqual(len(result["changed"]), 3)

    def test_ordering_only_difference_is_equivalent(self):
        """The exact N2-A.1 scenario: same warnings, different order in the
        raw text — must compare as fully equivalent."""
        root = str(SOURCE_ROOT)
        lines = _RAW_STDOUT_TEMPLATE.format(root=root).splitlines()
        warning_lines = [line for line in lines if ": warning " in line]
        order_a = "\n".join(warning_lines)
        order_b = "\n".join(reversed(warning_lines))
        pre = de.parse_warning_multiset(order_a, SOURCE_ROOT)
        post = de.parse_warning_multiset(order_b, SOURCE_ROOT)
        result = de.compare_multisets(pre, post)
        self.assertTrue(result["equivalent"])


if __name__ == "__main__":
    unittest.main()
