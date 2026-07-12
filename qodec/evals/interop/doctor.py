#!/usr/bin/env python3
"""doctor.py — the setup receipt.

Not a README that says "I think I ran it once": a machine-checked proof that
every tool a run will touch is actually present and answering. It verifies the
one hard requirement (a working qodec binary: encode -> decode is byte-exact
and a real tokenizer meter is active) and probes each optional optimizer,
printing a receipt the run pins into its metadata.

Exit status: 0 when qodec is healthy (optimizers may be absent — their lanes
just skip); non-zero only when qodec itself is broken, because then no lane can
run.

Usage:
    python3 doctor.py            # human summary
    python3 doctor.py --json     # machine receipt on stdout
"""

from __future__ import annotations

import argparse
import json
import sys

from adapters import OPTIMIZERS
from adapters import qodec as qodec_adapter

# The smallest payload that both mines and round-trips — proves the binary,
# the meter, and the container inverter in one shot.
_PROBE = (
    "src/a/Handler.cs:12: warning CS0168: unused\n"
    "src/a/Handler.cs:34: warning CS0168: unused\n"
    "src/a/Handler.cs:56: warning CS0168: unused\n"
    "src/a/Handler.cs:78: warning CS0168: unused\n"
)


def check_qodec() -> dict:
    receipt: dict = {"tool": "qodec", "ok": False}
    try:
        binary = qodec_adapter.binary()
        receipt["binary"] = str(binary)
        receipt["profile"] = binary.parent.name
        env = qodec_adapter.encode(_PROBE, passthrough=True)
        receipt["meter"] = env.meter
        receipt["probe_codec"] = env.codec
        receipt["probe_gain"] = round(env.gain, 4)
        back, _ = qodec_adapter.decode(env.content)
        receipt["roundtrip"] = back == _PROBE
        # A debug binary reports honest ratios but slanders wall-time; warn.
        if binary.parent.name != "release":
            receipt["warning"] = "debug binary — encode/decode timings are not representative"
        receipt["ok"] = receipt["roundtrip"] and env.meter != "approx"
        if env.meter == "approx":
            receipt["warning"] = "meter is `approx` (char heuristic), not a real BPE"
    except Exception as exc:  # noqa: BLE001 - doctor reports, never crashes
        receipt["error"] = str(exc)
    return receipt


def check_optimizers() -> list[dict]:
    out = []
    for name, mod in OPTIMIZERS.items():
        avail = mod.available()
        out.append({"tool": name, "ok": avail.ok, "detail": avail.detail})
    return out


def build_receipt() -> dict:
    qodec = check_qodec()
    return {
        "qodec": qodec,
        "optimizers": check_optimizers(),
        "healthy": qodec["ok"],
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--json", action="store_true", help="emit the receipt as JSON")
    args = ap.parse_args()

    receipt = build_receipt()
    if args.json:
        json.dump(receipt, sys.stdout, indent=2)
        sys.stdout.write("\n")
        return 0 if receipt["healthy"] else 1

    q = receipt["qodec"]
    mark = "ok" if q["ok"] else "FAIL"
    print(f"[{mark}] qodec")
    for key in ("binary", "profile", "meter", "probe_codec", "probe_gain", "roundtrip"):
        if key in q:
            print(f"       {key}: {q[key]}")
    if "warning" in q:
        print(f"       ! {q['warning']}")
    if "error" in q:
        print(f"       error: {q['error']}")
    print("optimizers (absent -> their lanes skip):")
    for row in receipt["optimizers"]:
        mark = "ok " if row["ok"] else "-- "
        print(f"  [{mark}] {row['tool']:<12} {row['detail']}")
    print()
    print("healthy" if receipt["healthy"] else "UNHEALTHY — qodec must work before any lane runs")
    return 0 if receipt["healthy"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
