"""Regression test for run_confined_build.sh's PRESERVE_ENV list.

Real N2-D1b evidence (CI run #7) found RUSTUP_TOOLCHAIN and VIRTUAL_ENV --
both correctly set by run_pilot_case.py and correctly listed in Sandboy's
own env_allow -- produced byte-identical failures to before those fixes
existed. Root cause: run_confined_build.sh (shared, unmodified, from N2-A's
dotnet-only canary) invokes `sudo --preserve-env=<hardcoded dotnet list>`,
which silently drops every non-dotnet env var before Sandboy's own
confinement ever sees it. This test pins the fix: PRESERVE_ENV must be a
superset covering every env_allow name across all 5 N2-D1b ecosystems.
"""
import sys
import unittest
from pathlib import Path

TOOLS = Path(__file__).resolve().parents[1] / "tools"
CANARY_TOOLS = Path(__file__).resolve().parents[2] / "canary" / "tools"
sys.path.insert(0, str(TOOLS))
import generic_sandbox_policy as gsp  # noqa: E402

RUN_CONFINED_BUILD_SH = CANARY_TOOLS / "run_confined_build.sh"


def _preserve_env_names() -> set[str]:
    text = RUN_CONFINED_BUILD_SH.read_text()
    line = next(row for row in text.splitlines() if row.strip().startswith("PRESERVE_ENV="))
    value = line.split("=", 1)[1].strip().strip('"')
    return set(value.split(","))


class TestPreserveEnvIsASupersetOfEveryEcosystem(unittest.TestCase):
    def test_every_ecosystem_env_allow_name_is_preserved_through_sudo(self):
        preserved = _preserve_env_names()
        for ecosystem, hints in gsp.ECOSYSTEM_POLICY_HINTS.items():
            for name in hints["env_allow"]:
                self.assertIn(
                    name, preserved,
                    f"{name!r} (needed by ecosystem {ecosystem!r}) is not in "
                    "run_confined_build.sh's PRESERVE_ENV -- sudo will silently "
                    "strip it before Sandboy's own env_allow ever sees it",
                )

    def test_original_dotnet_names_are_still_present(self):
        # This script is shared, unmodified-interface tooling with N2-A's own
        # frozen, already-accepted dotnet-only canary -- the fix must only
        # ever add names, never remove or rename the ones N2-A relies on.
        preserved = _preserve_env_names()
        for name in (
            "PATH", "HOME", "TMPDIR", "DOTNET_ROOT", "DOTNET_CLI_TELEMETRY_OPTOUT",
            "DOTNET_NOLOGO", "DOTNET_SKIP_FIRST_TIME_EXPERIENCE",
            "DOTNET_MULTILEVEL_LOOKUP", "DOTNET_GENERATE_ASPNET_CERTIFICATE",
        ):
            self.assertIn(name, preserved)


if __name__ == "__main__":
    unittest.main()
