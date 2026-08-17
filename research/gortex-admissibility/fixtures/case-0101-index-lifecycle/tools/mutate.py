#!/usr/bin/env python3
"""Scripted mutation steps for the index-lifecycle case.

A before/after snapshot pair would compare two pictures of the world. It would
not exercise the thing this case exists for: whether a tool notices the
TRANSITION and invalidates evidence it already handed out. So the states are
applied to a live tree instead.

Each state is defined ABSOLUTELY against the committed baseline, not as a delta
on the previous one. Applying s2 always yields the same tree whether or not s1
ran first, so a run can measure states in any order, repeat one, or resume after
a crash. The transition is still exercised, because the runner takes its
observation at s0 and only then applies a later state.

  tools/mutate.py list
  tools/mutate.py apply <state>
  tools/mutate.py reset
  tools/mutate.py selfcheck     apply every state, assert its structural
                                postcondition, reset -- validates the
                                instrument, invokes no tool under test

The first apply snapshots source/ into .baseline/ (git-ignored); reset restores
from it. No git dependency: the corpus must work when vendored elsewhere.
"""
import pathlib
import shutil
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
SOURCE = ROOT / "source"
BASELINE = ROOT / ".baseline"

REPO = SOURCE / "repo-main"
RATES = REPO / "src" / "core" / "rates.ts"
QUOTE = REPO / "src" / "app" / "quote.ts"

STATES = {
    "s0-fresh": "committed tree, indexed as-is; the observation is taken here",
    "s1-file-modified": "quote.ts gains a second call site after the observation",
    "s2-symbol-deleted": "RateTable.lookup and its call site are removed",
    "s3a-file-removed": "core/rates.ts disappears, leaving a dangling import",
    "s3b-repo-removed": "the whole repo-main root disappears",
}


def snapshot() -> None:
    if not BASELINE.exists():
        shutil.copytree(SOURCE, BASELINE)


def reset() -> None:
    if not BASELINE.exists():
        return
    if SOURCE.exists():
        shutil.rmtree(SOURCE)
    shutil.copytree(BASELINE, SOURCE)


def apply(state: str) -> None:
    snapshot()
    reset()

    if state == "s0-fresh":
        return

    if state == "s1-file-modified":
        QUOTE.write_text(
            QUOTE.read_text()
            + "\nexport function quoteBulk(table: RateTable, codes: string[]): number {\n"
            "  return codes.reduce((sum, c) => sum + table.lookup(c), 0);\n"
            "}\n"
        )
        return

    if state == "s2-symbol-deleted":
        RATES.write_text("export class RateTable {\n}\n")
        QUOTE.write_text(
            'import { RateTable } from "../core/rates";\n'
            "\n"
            "export function quote(table: RateTable, code: string): number {\n"
            "  return code.length * 100;\n"
            "}\n"
        )
        return

    if state == "s3a-file-removed":
        RATES.unlink()
        return

    if state == "s3b-repo-removed":
        shutil.rmtree(REPO)
        return

    raise SystemExit(f"unknown state: {state}")


def postcondition(state: str) -> tuple[bool, str]:
    """Structural assertion per state. Checks the mutation did what it says."""
    if state == "s0-fresh":
        ok = RATES.exists() and "lookup" in RATES.read_text() and QUOTE.read_text().count("lookup") == 1
        return ok, "rates.ts present with lookup; exactly one call site"
    if state == "s1-file-modified":
        ok = QUOTE.read_text().count("lookup") == 2 and RATES.exists()
        return ok, "two call sites; entity file untouched"
    if state == "s2-symbol-deleted":
        ok = RATES.exists() and "lookup" not in RATES.read_text() and "lookup" not in QUOTE.read_text()
        return ok, "lookup absent from both definition and call site"
    if state == "s3a-file-removed":
        ok = not RATES.exists() and QUOTE.exists() and "core/rates" in QUOTE.read_text()
        return ok, "definition file gone; import left dangling"
    if state == "s3b-repo-removed":
        ok = not REPO.exists()
        return ok, "repository root gone"
    return False, "no postcondition defined"


def main() -> int:
    if len(sys.argv) < 2 or sys.argv[1] == "list":
        for k, v in STATES.items():
            print(f"  {k:20s} {v}")
        return 0

    cmd = sys.argv[1]

    if cmd == "reset":
        reset()
        print("  mutate: source/ restored from baseline")
        return 0

    if cmd == "apply":
        if len(sys.argv) < 3:
            print("usage: mutate.py apply <state>", file=sys.stderr)
            return 2
        state = sys.argv[2]
        apply(state)
        ok, what = postcondition(state)
        print(f"  mutate: applied {state} -- {what} [{'ok' if ok else 'POSTCONDITION FAILED'}]")
        return 0 if ok else 1

    if cmd == "selfcheck":
        failed = 0
        for state in STATES:
            apply(state)
            ok, what = postcondition(state)
            print(f"  {'ok  ' if ok else 'FAIL'} {state:20s} {what}")
            failed += 0 if ok else 1
        reset()
        print(f"  mutate: {'all states apply cleanly' if not failed else f'{failed} state(s) broken'}; source/ reset")
        return 1 if failed else 0

    print(f"unknown command: {cmd}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
