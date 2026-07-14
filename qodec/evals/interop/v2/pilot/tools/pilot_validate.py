#!/usr/bin/env python3
"""Validate the Scope N1 public-development pilot corpus.

Checks (model-free, no tool execution): manifest + per-case schemas, the
public-development split invariant (zero validation/sealed cases), family/
ecosystem diversity, complete provenance + license + secret/PII review, that no
payload is hand-authored, shell/env safety of every recipe, bundle path safety,
derived (qodec/VG) leakage, committed snapshot-hash integrity, RTK-not-failed,
and that every structural anchor literal is present in its raw stream.

Use --inputs-only to validate authoring inputs before the canonical snapshots
have been captured by the pinned toolchain.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pilot_lib as pl  # noqa: E402

CORPUS_SCHEMAS = pl.V2_DIR / "corpus" / "schemas"
REQUIRED_MARKERS = {"PUBLIC-DEVELOPMENT", "PILOT", "NON-GATING",
                    "NOT-PUBLIC-VALIDATION", "NOT-SEALED-HELDOUT"}
FORBIDDEN_SPLIT_MARKERS = {"PUBLIC-VALIDATION", "SEALED-HELDOUT", "HELD-OUT", "SEALED"}


def _corpus_schema(name: str):
    return pl.load_json(CORPUS_SCHEMAS / name)


class Result:
    def __init__(self):
        self.violations: list[str] = []

    def fail(self, code: str, msg: str):
        self.violations.append(f"[{code}] {msg}")


def validate_manifest(m: dict, res: Result):
    for e in pl.js.validate(m, pl.load_schema("pilot-manifest.schema.json")):
        res.fail("schema", f"manifest: {e}")
    if m.get("case_count") != len(m.get("cases", [])):
        res.fail("count", f"case_count {m.get('case_count')} != len(cases) {len(m.get('cases', []))}")
    if len(m.get("cases", [])) != 10:
        res.fail("count", f"pilot requires exactly 10 cases, found {len(m.get('cases', []))}")
    if len(set(m.get("cases", []))) != len(m.get("cases", [])):
        res.fail("dup", "duplicate case ids in manifest")


def validate_case(case_id: str, res: Result, inputs_only: bool):
    b = pl.bundle_dir(case_id)
    if not (b / "case.json").exists():
        res.fail("missing-case", f"{case_id}: case.json missing")
        return None, None
    case = pl.load_json(b / "case.json")
    for e in pl.js.validate(case, pl.load_schema("pilot-case.schema.json")):
        res.fail("schema", f"{case_id} case.json: {e}")

    if case.get("split") != "public-development":
        res.fail("split", f"{case_id}: split {case.get('split')!r} is not public-development")
    if case.get("hand_authored") is not False:
        res.fail("hand-authored", f"{case_id}: payload marked hand_authored")
    markers = set(case.get("markers", []))
    if not REQUIRED_MARKERS.issubset(markers):
        res.fail("markers", f"{case_id}: missing markers {sorted(REQUIRED_MARKERS - markers)}")
    if markers & FORBIDDEN_SPLIT_MARKERS:
        res.fail("split-leak", f"{case_id}: forbidden split markers {sorted(markers & FORBIDDEN_SPLIT_MARKERS)}")

    # provenance + license (reuse frozen corpus provenance schema)
    prov_p = b / case.get("provenance_path", "provenance.json")
    if prov_p.exists():
        prov = pl.load_json(prov_p)
        for e in pl.js.validate(prov, _corpus_schema("provenance.schema.json")):
            res.fail("schema", f"{case_id} provenance: {e}")
        if not prov.get("license"):
            res.fail("license", f"{case_id}: provenance missing license")
        if not prov.get("secret_review"):
            res.fail("secret-review", f"{case_id}: provenance missing secret_review")
        if not prov.get("pii_review"):
            res.fail("pii-review", f"{case_id}: provenance missing pii_review")
        if prov.get("origin_kind") == "external-sanitized":
            up = (prov.get("upstream_revision") or "").lower()
            if not up or up in {"main", "master", "latest", "head", "trunk"}:
                res.fail("mutable-revision", f"{case_id}: external source needs immutable upstream_revision")
    else:
        res.fail("missing-provenance", f"{case_id}: provenance.json missing")

    # capture recipe (reuse frozen corpus recipe schema) + safety
    rec_p = b / case.get("capture_recipe_path", "capture-recipe.json")
    recipe = None
    if rec_p.exists():
        recipe = pl.load_json(rec_p)
        for e in pl.js.validate(recipe, _corpus_schema("capture-recipe.schema.json")):
            res.fail("schema", f"{case_id} capture-recipe: {e}")
        steps = recipe.get("setup", []) + [recipe.get("native", {}),
                                           {"argv": recipe.get("rtk", {}).get("argv", [])}]
        for step in steps:
            try:
                pl.capture.assert_argv_no_shell(step.get("argv") or [])
            except pl.capture.CaptureError as ce:
                res.fail("shell", f"{case_id}: {ce}")
        for name in recipe.get("environment_allowlist", []):
            if pl.capture.env_name_is_forbidden(name):
                res.fail("env-injection", f"{case_id}: forbidden env var {name}")
        if recipe.get("network_policy") != "disabled":
            res.fail("network", f"{case_id}: network_policy must be disabled")
        if recipe.get("expected_exit_code_class") == "exact" and "expected_exit_code" not in recipe:
            res.fail("exit-code", f"{case_id}: exact exit class requires expected_exit_code")
    else:
        res.fail("missing-recipe", f"{case_id}: capture-recipe.json missing")

    # anchors
    anc_p = b / case.get("anchors_path", "anchors.json")
    anchors = None
    if anc_p.exists():
        anchors = pl.load_json(anc_p)
        for e in pl.js.validate(anchors, pl.load_schema("pilot-anchors.schema.json")):
            res.fail("schema", f"{case_id} anchors: {e}")
    else:
        res.fail("missing-anchors", f"{case_id}: anchors.json missing")

    # bundle path safety + derived leakage (reuse frozen corpus snapshots helpers)
    for e in pl.snap.check_path_safety(b):
        res.fail("path", f"{case_id}: {e}")
    for e in pl.snap.check_derived_leakage(b):
        res.fail("qodec-leak", f"{case_id}: {e}")

    if inputs_only:
        return case, anchors

    # snapshot manifest + committed hash integrity
    sm_p = b / case.get("snapshot_manifest_path", "snapshot-manifest.json")
    if sm_p.exists():
        sm = pl.load_json(sm_p)
        for e in pl.js.validate(sm, pl.load_schema("pilot-snapshot-manifest.schema.json")):
            res.fail("schema", f"{case_id} snapshot-manifest: {e}")
        for e in pl.verify_snapshot_manifest(b, case, sm):
            res.fail("hash", f"{case_id}: {e}")
    else:
        res.fail("missing-snapshot-manifest", f"{case_id}: snapshot-manifest.json missing")

    # receipts present, phases, RTK not failed
    for phase, rp in (("native", pl.snap.NATIVE_RECEIPT), ("rtk", pl.snap.RTK_RECEIPT)):
        p = b / rp
        if not p.exists():
            res.fail("missing-receipt", f"{case_id}: {phase} receipt missing")
            continue
        r = pl.load_json(p)
        for e in pl.js.validate(r, _corpus_schema("execution-receipt.schema.json")):
            res.fail("schema", f"{case_id} {phase} receipt: {e}")
        if r.get("phase") != phase:
            res.fail("receipt", f"{case_id}: {phase} receipt phase mismatch")
        if phase == "rtk" and (r.get("exit_code") != 0 or r.get("rtk_classification") == "failed"):
            res.fail("rtk-failed", f"{case_id}: RTK failed (exit {r.get('exit_code')})")

    # anchors present in their raw stream (grounded in the raw payload)
    if anchors:
        for a in anchors.get("anchors", []):
            sfile = b / pl.STREAM_FILE.get(a["stream"], "")
            if not sfile.exists():
                res.fail("anchor-stream", f"{case_id}: anchor {a['anchor_id']} stream {a['stream']} missing")
                continue
            data = sfile.read_text(encoding="utf-8", errors="replace")
            if a["stream"].startswith("raw") and a["value"] not in data:
                res.fail("anchor-missing", f"{case_id}: anchor {a['anchor_id']} value not in {a['stream']}")
    return case, anchors


def main(argv=None) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--inputs-only", action="store_true")
    ap.add_argument("--case", default=None)
    args = ap.parse_args(argv)

    res = Result()
    manifest = pl.load_json(pl.MANIFEST_PATH)
    validate_manifest(manifest, res)
    ids = [args.case] if args.case else list(manifest.get("cases", []))
    families, ecosystems = set(), set()
    for cid in ids:
        case, _ = validate_case(cid, res, args.inputs_only)
        if case:
            families.add(case.get("family"))
            ecosystems.add(case.get("ecosystem"))
    if not args.case:
        if len(families) < manifest.get("min_families", 4):
            res.fail("diversity", f"only {len(families)} families (< {manifest.get('min_families')}): {sorted(families)}")
        if len(ecosystems) < manifest.get("min_ecosystems", 3):
            res.fail("diversity", f"only {len(ecosystems)} ecosystems (< {manifest.get('min_ecosystems')}): {sorted(ecosystems)}")

    if res.violations:
        print(f"PILOT CORPUS INVALID — {len(res.violations)} violation(s):", file=sys.stderr)
        for v in res.violations:
            print("  " + v, file=sys.stderr)
        return 1
    print(f"PILOT CORPUS VALID — {len(ids)} case(s), {len(families)} families, {len(ecosystems)} ecosystems")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
