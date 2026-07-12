#!/usr/bin/env python3
"""doctor.py — setup receipt CLI. See bench/doctor.py for the checks.

    python3 doctor.py                     # human summary of everything
    python3 doctor.py --strict rtk codegraph   # fail non-zero unless both are
                                          # pinned-version-matched, repos at
                                          # their SHA, and indexes ready
    python3 doctor.py --json              # machine receipt
"""

from __future__ import annotations

import argparse
import json
import sys

from bench import doctor


def _print_human(r: dict) -> None:
    q = r.get("qodec", {})
    print(f"[{'ok' if q.get('ok') else 'FAIL'}] qodec  {q.get('version','')}  meter={q.get('meter','?')}  "
          f"roundtrip={q.get('roundtrip','?')}  ({q.get('profile','?')})")
    if q.get("warning"):
        print(f"       ! {q['warning']}")
    if q.get("error"):
        print(f"       error: {q['error']}")
    print("tools:")
    for name, t in r.get("tools", {}).items():
        if t["kind"] == "unsupported":
            print(f"  [unsupported] {name:<12} {t.get('reason','')}")
        elif t["kind"] == "built":
            print(f"  [ok         ] {name:<12} built from crate")
        else:
            mark = "ok" if t.get("ok") else "FAIL"
            ver = f"{t.get('detected_version')} (pinned {t.get('pinned_version')})"
            match = "" if t.get("version_match") else "  VERSION MISMATCH"
            smoke = t.get("smoke", {})
            sm = f" smoke:{smoke.get('exit_code')}({smoke.get('elapsed_ms')}ms)" if smoke.get("argv") else ""
            print(f"  [{mark:<10}] {name:<12} {ver}{match}{sm}")
            if not t.get("ok") and t.get("reason"):
                print(f"                 {t['reason']}")
    if r.get("repos"):
        print("repos:")
        for rid, rr in r["repos"].items():
            mark = "ok" if rr.get("ok") else "FAIL"
            print(f"  [{mark:<10}] {rid:<12} HEAD={ (rr.get('head') or '-')[:12] }  pinned={rr['pinned_rev'][:12]}")
            if not rr.get("ok"):
                print(f"                 {rr.get('reason')}")
    if r.get("codegraph_indexes"):
        print("codegraph indexes:")
        for rid, ir in r["codegraph_indexes"].items():
            mark = "ok" if ir.get("ok") else "FAIL"
            print(f"  [{mark:<10}] {rid:<12} state={ir.get('index_state')} nodes={ir.get('node_count')} pending={ir.get('pending')}")
            if not ir.get("ok"):
                print(f"                 {ir.get('reason')}")
    if r.get("strict"):
        if r["strict_ok"]:
            print(f"\nstrict [{' '.join(r['strict'])}]: PASS")
        else:
            print(f"\nstrict [{' '.join(r['strict'])}]: FAIL")
            for f in r["strict_failures"]:
                print(f"  - {f}")
    print("\nhealthy" if r.get("healthy") else "\nUNHEALTHY — qodec must work before any lane runs")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--strict", nargs="+", default=[], metavar="TOOL",
                    help="required tools that must be pinned-matched + ready")
    ap.add_argument("--json", action="store_true")
    args = ap.parse_args()

    r = doctor.build_receipt(args.strict)
    if args.json:
        json.dump(r, sys.stdout, indent=2)
        sys.stdout.write("\n")
    else:
        _print_human(r)

    ok = r.get("healthy", False)
    if args.strict:
        ok = ok and r.get("strict_ok", False)
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
