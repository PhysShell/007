#!/usr/bin/env python3
"""N2-C eligibility dispatcher: routes each candidate to frozen N2-B
eligibility (unchanged) where it applies, or to the new N2-C
eligibility_extended rules where it doesn't. See n2b_bridge.py for the exact
boundary. This is the single entry point the rest of N2-C tooling calls —
callers never need to know which underlying rule set fired.
"""
from __future__ import annotations

import n2b_bridge
import eligibility_extended


def evaluate(candidate: dict) -> dict:
    if n2b_bridge.n2b_eligibility_applies(candidate):
        report = n2b_bridge.evaluate_via_n2b(candidate)
        report["rule_set"] = "n2b-frozen"
        return report
    report = eligibility_extended.evaluate(candidate)
    report["rule_set"] = "n2c-extended"
    return report


def evaluate_registry(registry: dict) -> list[dict]:
    return [evaluate(c) for c in registry["candidates"]]


if __name__ == "__main__":
    import json
    import sys
    from pathlib import Path

    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import registry as registry_mod  # noqa: E402

    reg = registry_mod.load_registry(Path(__file__).resolve().parents[1] / "candidate-registry.json")
    print(json.dumps(evaluate_registry(reg), indent=2), file=sys.stderr)
