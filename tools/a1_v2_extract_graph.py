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
import os
import re
import subprocess
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

# The locator's three fields. `blob` is the ONLY identity: a blob does not
# contain its own path, so proving `path_hint -> blob` would need a commit and a
# tree as well. `path_hint` is therefore named for what it is — a hint to a
# human — rather than dressed as a coordinate this layer can check. `anchor`
# selects a place inside the immutable bytes.
AUTHORITY_REF_FIELDS = ("blob", "path_hint", "anchor")

_OID = re.compile(r"[0-9a-f]{40}")


# The guard that makes "local only" true rather than assumed. In a partial clone
# (`remote.<name>.promisor=true`, e.g. `--filter=blob:none`) `git cat-file` will
# LAZILY FETCH a promised object it does not have. GIT_TERMINAL_PROMPT only
# suppresses credential prompting; it does not stop that fetch.
#
# Demonstrated, not assumed — against a local file:// promisor remote, asking for
# a blob genuinely absent from the object database:
#
#     without the guard   local-before=0   cat-file -t -> "blob"   (fetched)
#     with the guard      local-before=0   fatal: could not fetch …
#
# This matters precisely for `old_blob`, the half of a re-pin record that proves
# provenance: after a supersede it is exactly the object that has left the tree.
#
# `GIT_NO_REPLACE_OBJECTS=1` is the third guard, and it defends a different
# thing: not *where* git looks, but *what it hands back for a given name*. A
# `refs/replace/<oid>` ref makes `cat-file` return substitute bytes for the
# requested oid, silently and for both the type and the content query.
# Demonstrated:
#
#     git replace -f <authentic> <forged>
#     cat-file blob <authentic>                    -> "FORGED AUTHORITY …"
#     GIT_NO_REPLACE_OBJECTS=1 cat-file blob <..>  -> "AUTHENTIC AUTHORITY …"
#
# That is `blob` ceasing to be the identity of anything, which is the single
# assumption §2.5.1 rests on. Replace refs do not arrive over a default clone
# or fetch — verified — so the reach is a local checkout, i.e. exactly the
# reach of an ambient environment variable, and exactly the threat this layer
# is about.
# The fourth and fifth guards close git's CONFIGURATION scopes, which an empty
# environment does not. Reproduced: with `/etc/gitconfig` carrying a setting,
# `env -i git config --get user.name` returns it — the system scope is read from
# a fixed path and owes nothing to the environment. `GIT_CONFIG_NOSYSTEM=1`
# suppresses it.
#
# `GIT_CONFIG_GLOBAL=/dev/null` pins the remaining scope shut. This one is
# CONSTRUCTION, not a repaired leak, and the difference is worth keeping
# straight: git was tested here for a passwd-derived home when `HOME` is unset,
# and it did not read one. The pin is there so the claim — the child's
# configuration is exactly the scoped grant on the command line — is true by
# construction rather than by which platform happened to run the test.
#
# The repository's OWN config is still read, necessarily: without it the
# directory is not a repository. It cannot add an object directory (alternates
# are a file in the object store, not a config key), and this layer already
# asserts trust for exactly this path.
GIT_ENV = {"GIT_TERMINAL_PROMPT": "0", "GIT_NO_LAZY_FETCH": "1",
           "GIT_NO_REPLACE_OBJECTS": "1",
           "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": os.devnull}

# The child environment is CONSTRUCTED, not inherited. Copying `os.environ`
# wholesale made 135 ambient variables part of this extractor's input surface
# without declaring any of them, and two separate hazards rode on that — both
# demonstrated before this allowlist was written, neither assumed:
#
#   1. Credential widening. `ARLIAI_API_KEY`, exported in the shell that runs
#      the extractor, reached both `git cat-file` processes. AGENTS.md
#      invariant 1 rules that any new process able to read that key is a P0.
#      Object lookup needs no credential at all.
#   2. Object-store redirection. With `GIT_ALTERNATE_OBJECT_DIRECTORIES` set
#      ambiently, `git -C <repo> cat-file` answers with objects that are NOT in
#      <repo>: a blob written only into an unrelated repository resolved, and
#      its bytes came back. `GIT_DIR` and `GIT_OBJECT_DIRECTORY` are the same
#      hazard spelled differently. That makes *which bytes answer a provenance
#      question* a choice of whoever invokes the extractor — the one property
#      this layer exists to hold.
#
# Hazard 2 is why this is an allowlist and not a denylist of credential names:
# a denylist strips the names someone remembered, and the redirection family is
# open-ended in exactly that way.
#
# The allowlist is now EMPTY. It held `HOME`, so that git could read the global
# config where `safe.directory` is recorded — a checkout owned by another uid is
# refused with "detected dubious ownership" without it. EC-R1's review pointed
# out that this grant is far wider than the requirement that justified it: the
# need is "treat THIS repository as safe", and what was handed over was the
# whole home directory and every global git setting in it, credential-helper
# configuration and arbitrary `include.path` among them. Demonstrated — with
# `HOME` the child reads unrelated global state; with the grant below it does
# not.
#
# The requirement is therefore expressed where it belongs, in the command line,
# scoped to the one repository being read:
#
#     git -c safe.directory=<repo> -C <repo> cat-file …
#
# which succeeds under `env -i`. No home directory, no config file, no variable.
# Asserting trust for exactly this path grants nothing an attacker would not
# already hold: `repo` defaults to the checkout that contains this file, so
# anyone who owns it owns this program's source too.
GIT_ENV_ALLOW: tuple[str, ...] = ()

# `PATH` is NOT among them, and that is a third hazard rather than tidiness.
# While it was inherited, this module ran the unqualified command `git`, so the
# executable that reported the bytes was chosen by whoever set `PATH`.
# Demonstrated: a counterfeit `git` first on `PATH`, printing the genuine
# published blob, made `_local_object(PINNED_BLOB, repo=Path("/nonexistent"))`
# return 159462 bytes that satisfy the object-id check — for a repository that
# does not exist. Recomputing the id proves the bytes match the name; it cannot
# prove they came from an object database, because the process reporting them
# was the caller's.
#
# The gate's preflight is what gives this teeth: it spawns this extractor with
# `sys.executable`, an absolute path, so in that call the interpreter is trusted
# while `git` was not. Git is therefore looked up in a pinned list of absolute
# locations, and the child's `PATH` is set to the directory that answered —
# never inherited, and never silently falling back to `PATH` when none matches,
# since a silent fallback would restore exactly the hole it is closing.
GIT_CANDIDATES = ("/usr/bin/git", "/bin/git",
                  "/usr/local/bin/git", "/opt/homebrew/bin/git")


def _git_binary() -> str:
    """Resolve git from GIT_CANDIDATES. Never from `PATH`, never via `which`."""
    for path in GIT_CANDIDATES:
        if os.path.isfile(path) and os.access(path, os.X_OK):
            return path
    raise ExtractDefect(
        "NO TRUSTED GIT — none of " + ", ".join(GIT_CANDIDATES) + " is "
        "executable. This is not a reason to fall back to `PATH`: an object "
        "lookup answered by a caller-selected executable is not evidence. If "
        "git lives elsewhere here, that is a declared configuration change to "
        "GIT_CANDIDATES, not something this resolver may guess.")


def _git_env() -> dict[str, str]:
    """Build git's environment from GIT_ENV_ALLOW plus GIT_ENV. Never inherit."""
    env = {name: os.environ[name] for name in GIT_ENV_ALLOW if name in os.environ}
    env["PATH"] = str(Path(_git_binary()).parent)
    env.update(GIT_ENV)
    return env


def _local_object(oid: str, repo: Path = REPO) -> tuple[str, bytes] | None:
    """Look the object up in the LOCAL object database, without fetching.

    A deterministic extractor that occasionally goes to the internet for proof
    of its own authority would be an elegant way to lose the whole property —
    and until EB-R1's review this docstring claimed that outcome was impossible
    while the code permitted it in any partial clone. The claim is now enforced
    by GIT_ENV rather than asserted.

    "Local" is a claim about an object database, so it is also false if the
    caller gets to choose which database that is; `_git_env` is what keeps the
    answer a property of `repo`.

    And the returned bytes are CHECKED AGAINST THE NAME THEY WERE ASKED FOR.
    Each guard in GIT_ENV closes one way git can answer with something other
    than the cited object, and each was found one at a time, by a reviewer,
    after the previous one shipped. Recomputing the object id closes the
    *family*: whatever mechanism substitutes bytes — a replacement ref, an
    alternate object store, a future feature nobody here has read about — the
    substitute does not hash to the name. This is the E-R10 criterion applied
    to the extractor itself: the check is as discriminating as the distinction
    it enforces, instead of enumerating the ways the distinction can be lost.
    """
    env, git = _git_env(), _git_binary()
    # `-c safe.directory` is a CAPABILITY GRANT, written where it can be read.
    # It is scoped to this one path, it survives an empty environment, and it
    # replaces handing the child a home directory to go looking in.
    base = [git, "-c", f"safe.directory={Path(repo).resolve()}", "-C", str(repo)]
    kind = subprocess.run([*base, "cat-file", "-t", oid],
                          capture_output=True, text=True, encoding="utf-8",
                          errors="replace", env=env)
    if kind.returncode != 0:
        return None
    kind = kind.stdout.strip()
    body = subprocess.run([*base, "cat-file", kind, oid],
                          capture_output=True, env=env)
    # Not a locator defect and not "unresolvable": git answered, and answered
    # with bytes that are not the object asked for. No verdict computed on top
    # of this checkout would mean anything, so it aborts rather than returning
    # a value some caller might treat as merely absent.
    got = hashlib.sha1(b"%s %d\0" % (kind.encode(), len(body.stdout))
                       + body.stdout).hexdigest()
    _require(got == oid,
             f"OBJECT IDENTITY VIOLATED — asked git for {oid} and got bytes "
             f"hashing to {got} ({kind}, {len(body.stdout)} bytes). This "
             f"checkout substitutes object contents; nothing derived from it "
             f"is evidence of anything. Do NOT re-pin, and do NOT edit the "
             f"locator: the locator is the one part known to be intact.")
    return kind, body.stdout


def authority_ref_defect(ref: object) -> str | None:
    """Can the cited bytes, and a place within them, be identified? Nothing else.

    This is REFERENTIAL validation. It answers exactly one question:

        can I identify exactly the cited bytes, and one location inside them?

    It NEVER answers:

        do those bytes authorize the re-pin?

    So it deliberately does not check that the anchor says "supersede", that
    `blob` equals the entry's `new_blob`, that `path_hint` names the blob in any
    tree, that `old_blob` was legitimately superseded, or that `new_blob` is
    legitimate authority. Adding any one of those would put a small self-
    appointed lawyer inside this extractor, which is the layer violation §2.5
    exists to prevent — and the reason a *resolvable but normatively meaningless*
    anchor is required to PASS.

    Two failure states are distinguished, because conflating them would have
    this resolver lie about provenance in the course of improving it:

        MALFORMED LOCATOR          shape, oid syntax, object type, encoding
        UNRESOLVABLE IN THIS       the oid is well-formed and git cannot supply
        CHECKOUT                   the object locally

    The second does NOT mean the object is absent from the repository. Under the
    shallow clone CI uses by default, a perfectly real historical blob is simply
    not here, and proving absence would require the network.
    """
    if not isinstance(ref, dict):
        return (f"MALFORMED LOCATOR — superseding_authority is "
                f"{type(ref).__name__}, expected an object")
    missing = [f for f in AUTHORITY_REF_FIELDS if f not in ref]
    extra = sorted(k for k in ref if k not in AUTHORITY_REF_FIELDS)
    if missing or extra:
        return (f"MALFORMED LOCATOR — fields missing={missing} unexpected={extra}; "
                f"expected exactly {list(AUTHORITY_REF_FIELDS)}")
    bad_type = [f for f in AUTHORITY_REF_FIELDS
                if not isinstance(ref[f], str) or not ref[f]]
    if bad_type:
        return f"MALFORMED LOCATOR — {bad_type} must be non-empty strings"
    if not _OID.fullmatch(ref["blob"]):
        return (f"MALFORMED LOCATOR — blob {ref['blob']!r} is not a 40-character "
                "hex object id")

    found = _local_object(ref["blob"])
    if found is None:
        return (f"UNRESOLVABLE IN THIS CHECKOUT — object {ref['blob']} is not in "
                "the local object database. This does NOT assert the object is "
                "absent from the repository; establishing that would need the "
                "network, which this extractor never uses.")
    kind, raw = found
    if kind != "blob":
        return f"MALFORMED LOCATOR — {ref['blob']} is a {kind}, not a blob"
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        return f"MALFORMED LOCATOR — blob {ref['blob']} is not valid UTF-8"

    # OVERLAPPING start positions, not `str.count`. "exactly once" is a claim
    # about WHERE the anchor occurs, and `count` reports non-overlapping matches
    # only: in "aaa" it reports one occurrence of "aa" while two distinct start
    # positions exist. The frozen Phase G blob contains a real instance — a run
    # of 51 spaces, in which a 50-space anchor counts once and starts twice — so
    # an ambiguous locator would have resolved as unique.
    hits, at = 0, text.find(ref["anchor"])
    while at != -1:
        hits += 1
        at = text.find(ref["anchor"], at + 1)
    if hits == 0:
        return f"ANCHOR NOT FOUND — the cited text does not occur in {ref['blob']}"
    if hits > 1:
        return (f"ANCHOR AMBIGUOUS — the cited text occurs {hits} times in "
                f"{ref['blob']}; a locator must select one place")
    return None


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
        ref_defect = authority_ref_defect(e["superseding_authority"])
        if ref_defect is not None:
            return f"pin history entry {i} authority reference: {ref_defect}"
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


def _say(text: str, *, err: bool = False) -> None:
    """Write a report line as explicit UTF-8 bytes.

    `print` encodes with whatever codec stdout happens to carry. Under an ASCII
    locale (LC_ALL=C, PYTHONUTF8=0) every verdict string in this file raised
    UnicodeEncodeError on the em dash it contains — the run died mid-line with a
    truncated `RESULT: ` and Python's exit 1, which this contract reserves for
    FAIL. The gate's own prose, not the evidence, decided the verdict.

    E-R20 validated that INPUT strings encode to UTF-8. That was the wrong
    frame: reportability does not depend on the input's codec, it depends on the
    OUTPUT's, and stdout's codec is chosen by the environment. Encoding
    explicitly makes the report independent of it. (stderr survives by luck —
    Python defaults it to backslashreplace — which is not a property to rely on.)
    """
    stream = sys.stderr if err else sys.stdout
    buf = getattr(stream, "buffer", None)
    if buf is None:
        stream.write(text + "\n")
        return
    stream.flush()
    buf.write((text + "\n").encode("utf-8"))
    buf.flush()

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
        _say(f"wrote {args.out}: {len(derived['edges'])} edges, "
              f"{len(derived['event_kinds'])} event kinds")
        if derived["source_blob"] != PINNED_BLOB:
            _say(f"NOTE: source blob {derived['source_blob']} != pinned {PINNED_BLOB}")
        return 0

    committed = Path(args.out).read_text()
    if committed != text:
        _say("EXTRACT MISMATCH — the committed graph is not what the Phase G "
              "document derives to.", err=True)
        _say(f"  source blob : {derived['source_blob']}", err=True)
        _say(f"  pinned blob : {PINNED_BLOB}", err=True)
        return 1
    if derived["source_blob"] != PINNED_BLOB:
        _say(f"AUTHORITY MOVED — {SOURCE.name} is blob {derived['source_blob']}, "
              f"pinned {PINNED_BLOB}.", err=True)
        _say("This failure is EVIDENCE OF DRIFT, NOT AUTHORIZATION TO RE-PIN. "
              "A re-pin is legitimate only once Phase G has superseded the "
              "pinned artifact by its own mechanism; the ceremony is in "
              "docs/tasks/a1-f-v2-envelope.md 2.5.", err=True)
        return 1
    _say(f"extract OK: committed graph == derived, source blob {PINNED_BLOB}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
