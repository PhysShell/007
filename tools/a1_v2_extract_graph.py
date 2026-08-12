#!/usr/bin/env python3
"""Deterministic extractor for the frozen graph of FD-v2-GRAPH.

Reads the authoritative Phase G document and emits docs/tasks/a1-f-v2-graph.json.
The extract is not an editable registry: `--check` regenerates it from the
document and fails if the committed JSON differs by one byte, so a drafter
cannot relax a gate by editing its right-hand side.

The source document is pinned by blob, not by path alone. A path can be
rewritten in place; a blob cannot.

Moving that pin is a defined ceremony, not an edit (see the Envelope v2 document
2.5). PINNED_BLOB is an EVIDENCE-BINDING update, not a new authority decision:
legitimacy is decided one floor up, by Phase G superseding its own artifact
through its own normative mechanism, and the re-pin merely binds this
implementation to the result. A --check failure is therefore evidence of drift
and never authorization to re-pin; otherwise the property degrades into "the
hash changed, so update the hash".

Usage:
    python3 tools/a1_v2_extract_graph.py --write     # regenerate the artifact
    python3 tools/a1_v2_extract_graph.py --check     # verify committed == derived
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
SOURCE = REPO / "docs/tasks/a1-f-v2-phase-g.md"
OUT = REPO / "docs/tasks/a1-f-v2-graph.json"

# Phase G as merged in PR #128. Recorded so the extractor notices if the
# authority it claims to derive from has moved underneath it.
PINNED_BLOB = "450380ff0d1f8ec08f783968f08bc6b3942f44a5"

# The original pin, which is never edited. It is what makes an undocumented
# re-pin visible: with no history recorded, PINNED_BLOB must still be this.
ORIGINAL_BLOB = "450380ff0d1f8ec08f783968f08bc6b3942f44a5"

# Re-pin evidence, append-only BY CONTRACT. One entry per legitimate supersede,
# each naming the blob it replaced, the blob it installed, and the Phase G
# supersede that authorized the move. Empty means the original pin has never
# moved. Append-only is the rule for whoever edits this list, not a property
# proved below: the check sees one snapshot, so a rewritten old entry with
# repaired links reads as a valid chain.
PIN_HISTORY: list[dict[str, str]] = []

PIN_FIELDS = ("old_blob", "new_blob", "superseding_authority")


def pin_chain_defect(pinned: str, history: list[dict]) -> str | None:
    """Is the pin's evidence well-formed? Returns the defect, or None.

    This checks BINDING, not legitimacy. Whether Phase G may be superseded at
    all is not decidable here and is not attempted: that is a contract act,
    recorded by Phase G's own mechanism. What is decidable here is whether a pin
    arrived without its paperwork, whether paperwork was filed that moved no
    pin, and whether the recorded chain actually runs from the original blob to
    the pinned one. A re-pin thus cannot be a one-token edit; it is a structured
    change that names what it replaced and on whose authority.

    Its reach is ONE SNAPSHOT, and the claim is scoped to match: the pin and its
    transition record must be jointly valid in every accepted checked state.
    That is tree-state joint validity - NOT commit atomicity, and NOT historical
    immutability. A pin moved in one commit and papered in the next leaves a
    passing HEAD, and a rewritten old entry with repaired links reads as a valid
    chain. Catching either needs an external frozen witness (git history, a
    countersigned record) that this floor does not have and is not given.

    It does not defend against an editor who rewrites this file's own constants.
    Nothing self-hosted can. It makes the attempt legible in a diff, which is
    where the defence actually lives.
    """
    for i, e in enumerate(history):
        missing = [f for f in PIN_FIELDS if not e.get(f)]
        if missing:
            return f"pin history entry {i} does not record {missing}"
        prev = history[i - 1]["new_blob"] if i else ORIGINAL_BLOB
        if e["old_blob"] != prev:
            return (f"pin history entry {i} replaces {e['old_blob']}, but the "
                    f"blob in force at that point was {prev}")
    installed = history[-1]["new_blob"] if history else ORIGINAL_BLOB
    if pinned != installed:
        return (f"PINNED_BLOB is {pinned}, but the recorded evidence installs "
                f"{installed} — a re-pin and its evidence must land together")
    return None


class ExtractDefect(RuntimeError):
    """A frozen fact the Phase G document must satisfy did not hold."""


def _require(condition: object, message: str) -> None:
    """Every structural check below MUST use this, never `assert`.

    Python strips `assert` when -O is set or PYTHONOPTIMIZE is non-empty. That
    made the whole of this module optional: with PYTHONOPTIMIZE=1 a drafter
    could edit PINNED_BLOB, leave PIN_HISTORY empty, run --write then --check,
    and be told `extract OK` - the exact one-token re-pin the ceremony in
    docs/tasks/a1-f-v2-envelope.md 2.5 exists to block. The 69-edge, 56/13,
    21/11 and 47/20 facts evaporated with it. A check that an environment
    variable can delete is not a check.
    """
    if not condition:
        raise ExtractDefect(message)


def _camel(snake: str) -> str:
    return "".join(w.capitalize() for w in snake.split("_"))


def parse_meta_members(text: str) -> list[str]:
    """The eleven members, read from 4.2.5's frozen membership block."""
    m = re.search(r"members := exactly the eleven envelope-bearing message kinds"
                  r".*?\n\n(.*?)\n\nresolution", text, re.S)
    _require(m, "membership block not found in the Phase G document")
    members = [_camel(t) for t in re.findall(r"[a-z][a-z_]+", m.group(1))]
    _require(len(members) == 11, f"expected 11 meta-target members, found {len(members)}")
    return members


def parse_typed_supports(text: str) -> list[str]:
    """The four typed supports, read from 4.2.5's typed-universe sentence."""
    m = re.search(r"four typed\s+supports \((.*?)\)", text, re.S)
    _require(m, "typed-supports enumeration not found in the Phase G document")
    supports = re.findall(r"`(\w+)`", m.group(1))
    _require(len(supports) == 4, f"expected 4 typed supports, found {len(supports)}")
    return supports


def git_blob_sha(data: bytes) -> str:
    return hashlib.sha1(b"blob %d\x00" % len(data) + data).hexdigest()


def extract(source: Path) -> dict:
    defect = pin_chain_defect(PINNED_BLOB, PIN_HISTORY)
    _require(defect is None, f"PIN EVIDENCE DEFECT — {defect}")

    raw = source.read_bytes()
    text = raw.decode("utf-8")
    lines = text.split("\n")

    # Derived from the document, not transcribed. A hand-kept copy of a frozen
    # set is a second registry, and this one is load-bearing: the meta-target
    # members decide what a subject_refs field is allowed to carry.
    message_kinds = parse_meta_members(text)
    typed_supports = parse_typed_supports(text)
    meta_targets = {"AnyCommittedEnvelope": message_kinds}

    start = next(i for i, l in enumerate(lines)
                 if l.startswith("| # | source semantic kind"))
    edges = []
    for line in lines[start + 2:]:
        if not line.startswith("|"):
            break
        c = [x.strip().strip("`") for x in line.strip("|").split("|")]
        edges.append({"n": int(c[0]), "source": c[1], "target": c[2],
                      "class": c[3], "origin": c[4]})

    # Structural assertions. These are the frozen numbers; if the document ever
    # disagrees with them the extractor stops rather than emitting a quieter graph.
    _require(edges, "no registry table found")
    _require([e["n"] for e in edges] == list(range(1, len(edges) + 1)), "row numbers not contiguous")
    cls = collections.Counter(e["class"] for e in edges)
    _require(len(edges) == 69, f"expected 69 edges, found {len(edges)}")
    _require(cls["Intra"] == 56 and cls["Causal"] == 13, f"class split moved: {dict(cls)}")
    _require(len({(e["source"], e["target"], e["class"]) for e in edges}) == 69, "duplicate edge")

    event_kinds = sorted({re.fullmatch(r"CampaignEvent\((\w+)\)", e["source"]).group(1)
                          for e in edges if e["source"].startswith("CampaignEvent(")})
    payload_variants = sorted({e["target"][:-len("Payload")] for e in edges
                               if e["source"].startswith("CampaignEvent(")
                               and e["target"].endswith("Payload")})
    _require(len(event_kinds) == 21, f"expected 21 event kinds, found {len(event_kinds)}")
    _require(len(payload_variants) == 11, f"expected 11 payload variants, found {len(payload_variants)}")

    expected_payload = {k: (k + "Payload" if k in payload_variants else None)
                        for k in event_kinds}
    bearing = sum(1 for v in expected_payload.values() if v)
    _require(bearing == 11 and len(expected_payload) - bearing == 10, "presence map is not 11/10")

    typed = (set(message_kinds) | set(typed_supports)
             | {p + "Payload" for p in payload_variants}
             | {f"CampaignEvent({k})" for k in event_kinds})
    nodes = set()
    for e in edges:
        nodes |= {e["source"], e["target"]}
    terminal = sorted(nodes - typed - set(meta_targets))
    _require(len(typed) == 47, f"expected 47 typed nodes, found {len(typed)}")
    _require(len(terminal) == 20, f"expected 20 terminal kinds, found {len(terminal)}")
    _require(not (set(meta_targets) & {e["source"] for e in edges}), "a meta-target appears as an edge SOURCE; it is a target-position union only")

    return {
        "_comment": ("Frozen graph of FD-v2-GRAPH. GENERATED by "
                     "tools/a1_v2_extract_graph.py from the Phase G document; "
                     "do not hand-edit. Verify with --check."),
        "source_document": str(SOURCE.relative_to(REPO)),
        "source_blob": git_blob_sha(raw),
        "pinned_blob": PINNED_BLOB,
        "original_blob": ORIGINAL_BLOB,
        "pin_history": PIN_HISTORY,
        "status": "APPROVED / CLOSED",
        "message_kinds": message_kinds,
        "typed_supports": typed_supports,
        "event_kinds": event_kinds,
        "payload_variants": payload_variants,
        "expected_payload": expected_payload,
        "meta_targets": meta_targets,
        "terminal_kinds": terminal,
        "edges": edges,
    }


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true")
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--source", default=str(SOURCE))
    ap.add_argument("--out", default=str(OUT))
    args = ap.parse_args()
    if not (args.write or args.check):
        ap.error("choose --write or --check")

    derived = extract(Path(args.source))
    text = json.dumps(derived, indent=2) + "\n"

    if args.write:
        Path(args.out).write_text(text)
        print(f"wrote {args.out}: {len(derived['edges'])} edges, "
              f"{len(derived['event_kinds'])} event kinds")
        if derived["source_blob"] != PINNED_BLOB:
            print(f"NOTE: source blob {derived['source_blob']} != pinned {PINNED_BLOB}")
        return 0

    committed = Path(args.out).read_text()
    if committed != text:
        print("EXTRACT MISMATCH — the committed graph is not what the Phase G "
              "document derives to.", file=sys.stderr)
        print(f"  source blob : {derived['source_blob']}", file=sys.stderr)
        print(f"  pinned blob : {PINNED_BLOB}", file=sys.stderr)
        return 1
    if derived["source_blob"] != PINNED_BLOB:
        print(f"AUTHORITY MOVED — {SOURCE.name} is blob {derived['source_blob']}, "
              f"pinned {PINNED_BLOB}.", file=sys.stderr)
        print("This failure is EVIDENCE OF DRIFT, NOT AUTHORIZATION TO RE-PIN. "
              "A re-pin is legitimate only once Phase G has superseded the "
              "pinned artifact by its own mechanism; the ceremony is in "
              "docs/tasks/a1-f-v2-envelope.md 2.5.", file=sys.stderr)
        return 1
    print(f"extract OK: committed graph == derived, source blob {PINNED_BLOB}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
