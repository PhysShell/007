#!/usr/bin/env python3
"""N2-B BuildDetector (section 8).

Dispatches to every registered ToolAdapter's detect() (filenames/manifests/
lockfiles only — never restore/build/test/script execution) and reports,
per ecosystem, a deterministic detection result. If more than one ecosystem
adapter reports a plausible entry point, automatic ecosystem selection is
refused — same "ambiguous requires explicit manifest selection" rule as
within a single ecosystem's multiple entry points.
"""
from __future__ import annotations

from pathlib import Path

from adapters import ADAPTERS


def detect_all(source_root: Path) -> dict:
    source_root = Path(source_root)
    per_ecosystem = {}
    for ecosystem, adapter in ADAPTERS.items():
        result = adapter.detect(source_root)
        if result["candidate_entry_points"]:
            per_ecosystem[ecosystem] = result

    plausible = [eco for eco, r in per_ecosystem.items() if r["confidence"] > 0]
    ambiguous_ecosystem_selection = len(plausible) > 1

    return {
        "source_root": str(source_root),
        "ecosystems_detected": sorted(per_ecosystem),
        "per_ecosystem": per_ecosystem,
        "ambiguous_ecosystem_selection": ambiguous_ecosystem_selection,
        "requires_explicit_ecosystem_selection": ambiguous_ecosystem_selection,
    }


if __name__ == "__main__":
    import json
    import sys

    report = detect_all(Path(sys.argv[1]))
    print(json.dumps(report, indent=2))
