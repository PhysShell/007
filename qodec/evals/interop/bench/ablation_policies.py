"""Factorized alias × structural ablation policies (eval-only).

Six arms probe how the production `squeeze` codec's factors relate to a loss:

    R   raw+brief                      no container
    I   %q1 identity container         framing only
    M   mine over raw                  aliasing only, no structural stage
    F   structural (fold/grep)         verbatim structural only, no aliases
    MF  squeeze  (== production)        production shelf + mine
    VG  fold-grep-guarded              fold/grep shelf + GUARDED mine

IMPORTANT — VG is NOT "guarded squeeze". Production squeeze's structural stage is
`toon | best(fold,grep,diag,tmpl)`; VG's is `best(fold,grep)` ONLY, then a guarded
mine. So VG differs from MF in TWO ways (a smaller structural shelf AND the mine
guard), and an `MF fail / VG pass` flip is candidate-policy evidence, not a clean
lexical-guard attribution. A stage-matched S/SM/SG comparison (Commit I) isolates
the guard. Production `squeeze` is untouched; MF calls it and reproduces it
byte-for-byte. The guard is generic surface-only (no task/gold) — NOT protected
spans.
"""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
from dataclasses import dataclass

# Arms as (name, codec, alias_intent, structural_intent, guard). R has no codec.
# "intent" is the POLICY; the realized_stages() receipt records what was actually
# applied (never inferred from the arm name).
POLICIES = [
    ("R", None, False, False, False),
    ("I", "identity", False, False, False),
    ("M", "mine", True, False, False),
    ("F", "structural", False, True, False),
    ("MF", "squeeze", True, True, False),
    ("VG", "fold-grep-guarded", True, True, True),
]
ARM_NAMES = [p[0] for p in POLICIES]

# Stage-matched CLOSURE arms (Commit I): S = production stage-1 only; SM = squeeze
# (== production); SG = production stage-1 + guarded mine (SM and SG share the
# exact stage-1 and differ ONLY in the guard); V/VG = the fold/grep-only shelf.
CLOSURE_POLICIES = [
    ("R", None, False, False, False),
    ("S", "squeeze-stage1", False, True, False),
    ("SM", "squeeze", True, True, False),
    ("SG", "squeeze-mine-guarded", True, True, True),
    ("V", "structural", False, True, False),
    ("VG", "fold-grep-guarded", True, True, True),
]
CLOSURE_ARMS = [p[0] for p in CLOSURE_POLICIES]

PROD_SHELF = ["toon", "fold", "grep", "diag", "tmpl"]
# The structural candidate shelf each arm's codec may pick from (for the receipt).
SHELF = {"R": [], "I": [], "M": [], "F": ["fold", "grep"], "MF": PROD_SHELF, "VG": ["fold", "grep"],
         "S": PROD_SHELF, "SM": PROD_SHELF, "SG": PROD_SHELF, "V": ["fold", "grep"]}


def _sha(s: str) -> str:
    return hashlib.sha256(s.encode("utf-8")).hexdigest()


def is_guarded_lexical(phrase: str) -> bool:
    """Python mirror of qodec's mine::is_guarded_lexical — used only to VERIFY
    that GF never aliased a guarded span. Must stay in lockstep with the Rust."""
    if any(c in phrase for c in ("`", "»")) or "::" in phrase or "/" in phrase:
        return True
    if any(ext in phrase for ext in
           (".rs", ".cs", ".md", ".toml", ".json", ".lock", ".jinja", ".py", ".txt", ".yaml")):
        return True
    for i in range(1, len(phrase)):
        if phrase[i].isupper() and phrase[i - 1].islower() and phrase[i - 1].isascii():
            return True
        if phrase[i] == "_" and phrase[i - 1].isalnum() and i + 1 < len(phrase) and phrase[i + 1].isalnum():
            return True
    return False


def legend_of(artifact: str) -> dict:
    """The alias legend (glyph=phrase lines) of a %q1 container, if any."""
    lines = artifact.split("\n")
    if not lines or not lines[0].startswith("%q1 "):
        return {}
    legend = {}
    for ln in lines[1:]:
        if ln.startswith("%q1 body"):
            break
        if "=" in ln:
            a, phrase = ln.split("=", 1)
            if a:
                legend[a] = phrase
    return legend


@dataclass
class ArmResult:
    arm: str
    artifact: str            # exactly what the model reads for this arm's payload
    encoded: bool
    tokens: int
    roundtrip_ok: bool
    receipt: dict


def _encode(qodec_bin: str, codec: str, text: str, meter: str) -> dict:
    out = subprocess.run([qodec_bin, "encode", "--codec", codec, "--meter", meter, "--json"],
                         input=text, capture_output=True, text=True, check=True)
    return json.loads(out.stdout)


def _decode(qodec_bin: str, content: str) -> str:
    out = subprocess.run([qodec_bin, "decode"], input=content, capture_output=True, text=True, check=True)
    return out.stdout


def apply_policy(policy, raw_payload: str, meter: str, qodec_bin: str) -> ArmResult:
    """Produce one arm's payload + stage receipt from the raw tool payload. No
    passthrough: encoded arms always emit their container so the treatment is
    actually applied (I would otherwise vanish, since framing never 'pays')."""
    name, codec, alias, structural, guard = policy
    if codec is None:                                   # R — raw, no container
        artifact = raw_payload
        receipt = {"alias_enabled": False, "structural_enabled": False, "lexical_guard": False,
                   "format_codec": None, "miner": None,
                   "artifact_sha256": _sha(artifact), "roundtrip_sha256": _sha(artifact),
                   "tokens": None}
        return ArmResult(name, artifact, False, receipt["tokens"], True, receipt)

    env = _encode(qodec_bin, codec, raw_payload, meter)
    artifact = env["content"]
    encoded = env["encoded"]
    container_codec = artifact.split("\n", 1)[0].split()[1] if encoded and artifact.startswith("%q1 ") else "raw"
    roundtrip = _decode(qodec_bin, artifact) if encoded else artifact
    receipt = {
        "alias_enabled": alias,
        "structural_enabled": structural,
        "lexical_guard": guard,
        "format_codec": container_codec,
        "miner": "mine" if alias else None,
        "artifact_sha256": _sha(artifact),
        "roundtrip_sha256": _sha(roundtrip),
        "tokens": env["tokens_out"],
    }
    return ArmResult(name, artifact, encoded, env["tokens_out"], roundtrip == raw_payload, receipt)


def encode_all_arms(raw_payload: str, meter: str, qodec_bin: str, policies=None) -> dict:
    policies = policies or POLICIES
    return {p[0]: apply_policy(p, raw_payload, meter, qodec_bin) for p in policies}


_BY_NAME = {p[0]: p for p in POLICIES}
_BY_NAME.update({p[0]: p for p in CLOSURE_POLICIES})


def _outer_codec(artifact: str) -> str:
    return artifact.split("\n", 1)[0].split()[1] if artifact.startswith("%q1 ") else "raw"


def _tokens(qodec_bin: str, codec: str, text: str, meter: str) -> int:
    return _encode(qodec_bin, codec, text, meter)["tokens_out"]


# Per-arm stage execution, from the POLICY (not the arm name string): the codec
# that produces stage-1 (None ⇒ the mine runs over the raw text; "identity" ⇒ a
# no-transform frame), whether the policy invokes a mine stage, and its guard.
STAGE_META = {
    "R": (None, False, False),
    "I": ("identity", False, False),
    "M": (None, True, False),
    "F": ("structural", False, False),
    "MF": ("squeeze-stage1", True, False),
    "VG": ("structural", True, True),
    "S": ("squeeze-stage1", False, False),
    "SM": ("squeeze-stage1", True, False),
    "SG": ("squeeze-stage1", True, True),
    "V": ("structural", False, False),
}
_NO_TRANSFORM_CODECS = {"raw", "identity", None}


# A run_reader-style codec maps to the same (stage1_codec, mines, guard) triple,
# so realized stages can be computed for a promotion run driven by --codec.
CODEC_STAGE = {
    "fold-grep-guarded": ("structural", True, True),   # VG
    "squeeze": ("squeeze-stage1", True, False),
    "squeeze-mine-guarded": ("squeeze-stage1", True, True),
    "squeeze-stage1": ("squeeze-stage1", False, False),
    "structural": ("structural", False, False),
    "mine": (None, True, False),
    "deep": (None, True, False),
    "identity": ("identity", False, False),
}
CODEC_POLICY_LABEL = {"fold-grep-guarded": "VG", "squeeze": "MF", "squeeze-mine-guarded": "SG",
                      "squeeze-stage1": "S", "structural": "V", "mine": "M", "identity": "I"}


def realized_stages(arm: str, raw_payload: str, meter: str, qodec_bin: str) -> dict:
    """Realized codec stages of an ablation ARM (uses its policy metadata)."""
    stage1_codec, mines, guard = STAGE_META[arm]
    res = apply_policy(_BY_NAME[arm], raw_payload, meter, qodec_bin)
    return _realized(arm, res.artifact, res.tokens, res.encoded,
                     stage1_codec, mines, guard, raw_payload, meter, qodec_bin)


def realized_stages_for_codec(codec: str, raw_payload: str, meter: str, qodec_bin: str,
                              passthrough: bool = False) -> dict:
    """Realized codec stages for a promotion run driven by a --codec (e.g. VG =
    fold-grep-guarded), computed against the artifact the reader actually gets."""
    if codec not in CODEC_STAGE:
        raise ValueError(f"no stage metadata for codec {codec!r}")
    stage1_codec, mines, guard = CODEC_STAGE[codec]
    env = _encode(qodec_bin, codec, raw_payload, meter) if not passthrough else \
        json.loads(subprocess.run([qodec_bin, "encode", "--codec", codec, "--meter", meter,
                                   "--json", "--passthrough-on-no-gain"],
                                  input=raw_payload, capture_output=True, text=True, check=True).stdout)
    return _realized(CODEC_POLICY_LABEL.get(codec, codec), env["content"], env["tokens_out"],
                     env["encoded"], stage1_codec, mines, guard, raw_payload, meter, qodec_bin)


def _realized(label, final_art, final_tokens, final_encoded, stage1_codec, mines, guard,
              raw_payload, meter, qodec_bin) -> dict:
    """Shared core: the REALIZED stages read from the artifacts — never inferred
    from a name or from the final legend (a `tmpl`/`diag` stage-1 carries a legend
    without any mining). stage-2 execution comes from the policy; stage-2
    transform from the SHA changing between stage-1 and final."""
    arm = label
    final_codec = _outer_codec(final_art) if final_encoded else None
    final_legend = legend_of(final_art)

    # ---- stage 1 (structural) --------------------------------------------- #
    if stage1_codec is None:
        # No structural stage: the mine (if any) runs over the raw text itself.
        s1_text, s1_codec, s1_legend = raw_payload, "raw", {}
        stage1 = {"attempted": False, "selected_codec": None, "transform_applied": False,
                  "alias_entries": 0, "artifact_sha256": None, "tokens": None}
        s2_input_sha = _sha(raw_payload)
    else:
        s1 = _encode(qodec_bin, stage1_codec, raw_payload, meter)
        s1_text = s1["content"]
        s1_codec = _outer_codec(s1_text) if s1["encoded"] else "raw"
        s1_legend = legend_of(s1_text)
        stage1 = {"attempted": True, "selected_codec": s1_codec,
                  "transform_applied": s1_codec not in _NO_TRANSFORM_CODECS,
                  "alias_entries": len(s1_legend),
                  "artifact_sha256": _sha(s1_text), "tokens": s1["tokens_out"]}
        s2_input_sha = _sha(s1_text)

    # ---- stage 2 (mine) --------------------------------------------------- #
    if not mines:
        stage2 = {"attempted": False, "selected_miner": None, "transform_applied": False,
                  "alias_entries_added": 0, "input_artifact_sha256": None,
                  "artifact_sha256": None, "tokens": None}
    else:
        transform = _sha(final_art) != s2_input_sha
        miner = None
        if transform and not guard:
            # Identify which unguarded miner produced the final (mine_over picks
            # the cheaper). Guarded miners have no standalone codec, so we leave
            # the family unresolved rather than guess.
            for cand in ("mine", "deep"):
                if _encode(qodec_bin, cand, s1_text, meter)["content"] == final_art:
                    miner = cand
                    break
            miner = miner or "mine-or-deep"
        elif transform:
            miner = "guarded-mine-or-deep"
        stage2 = {"attempted": True, "selected_miner": miner, "transform_applied": transform,
                  "alias_entries_added": len(final_legend) - len(s1_legend),
                  "input_artifact_sha256": s2_input_sha,
                  "artifact_sha256": _sha(final_art), "tokens": final_tokens}

    return {
        "arm": arm,
        "stage1": stage1,
        "stage2": stage2,
        "final": {"outer_codec": final_codec, "artifact_sha256": _sha(final_art), "tokens": final_tokens},
        "overall_alias_entries": len(final_legend),
        "alias_applied": bool(final_legend),
        "structural_applied": stage1["transform_applied"],
    }


def _is_json(text: str) -> bool:
    try:
        json.loads(text)
        return True
    except Exception:
        return False


def stage_match_violations(receipts: dict) -> list[str]:
    """Per case, verify the closure comparison is stage-matched: S's final, SM's
    stage-2 input and SG's stage-2 input are the SAME stage-1 artifact, and their
    selected stage-1 codec agrees; and SM/SG differ only by the mine guard
    (policy-level). Any violation must abort the causal verdict."""
    viol = []
    cases = sorted({k.split("|")[0] for k in receipts})
    for case in cases:
        S, SM, SG = receipts.get(f"{case}|S"), receipts.get(f"{case}|SM"), receipts.get(f"{case}|SG")
        if not (S and SM and SG):
            continue
        shas = {S["final"]["artifact_sha256"], SM["stage2"]["input_artifact_sha256"],
                SG["stage2"]["input_artifact_sha256"]}
        if len(shas) != 1:
            viol.append(f"{case}: stage-1 SHA differs across S.final / SM.input / SG.input")
        codecs = {S["stage1"]["selected_codec"], SM["stage1"]["selected_codec"], SG["stage1"]["selected_codec"]}
        if len(codecs) != 1:
            viol.append(f"{case}: stage-1 selected codec differs across S/SM/SG ({codecs})")
    # SM and SG must differ only by the guard at the policy level.
    if STAGE_META["SM"][0] != STAGE_META["SG"][0] or STAGE_META["SM"][1] != STAGE_META["SG"][1] \
            or STAGE_META["SM"][2] == STAGE_META["SG"][2]:
        viol.append("SM and SG policy configs differ by more than the mine guard")
    return viol


def byte_identical_pairs(arms: dict) -> list[tuple]:
    """Arm pairs whose artifacts are byte-identical (e.g. F and VG when the guard
    rejected every candidate the structural stage left). Reported so a 'both pass'
    is not read as two independent results."""
    names = [n for n in ARM_NAMES if arms[n].encoded]
    out = []
    for i, a in enumerate(names):
        for b in names[i + 1:]:
            if arms[a].artifact == arms[b].artifact:
                out.append((a, b))
    return out


# --------------------------------------------------------------------------- #
# Invariant checks — a policy that violates one must never enter a run.
# --------------------------------------------------------------------------- #

def check_invariants(arms: dict, raw_payload: str, squeeze_artifact: str | None = None) -> list[str]:
    """Return a list of invariant violations (empty = all hold)."""
    viol = []
    for name, res in arms.items():
        if not res.roundtrip_ok:
            viol.append(f"{name}: roundtrip not byte-exact")
    # M-off arms carry no aliases.
    for name in ("R", "I", "F"):
        if legend_of(arms[name].artifact):
            viol.append(f"{name}: alias legend present in an alias-off arm")
    # F keeps full paths verbatim.
    for tok in set(re.findall(r"[\w./-]+\.(?:rs|cs|md|toml|json|lock|py)", raw_payload)):
        if tok not in arms["F"].artifact:
            viol.append(f"F: path token not verbatim: {tok}")
            break
    # VG never aliases a guarded span.
    for a, phrase in legend_of(arms["VG"].artifact).items():
        if is_guarded_lexical(phrase):
            viol.append(f"VG: aliased a guarded span {a}={phrase!r}")
            break
    # MF reproduces production squeeze byte-for-byte.
    if squeeze_artifact is not None and arms["MF"].artifact != squeeze_artifact:
        viol.append("MF: does not reproduce production squeeze byte-for-byte")
    # I is a real container (framing applied), not a passthrough.
    if not arms["I"].artifact.startswith("%q1 identity"):
        viol.append("I: not an identity container")
    return viol


def check_closure_invariants(arms: dict, raw_payload: str, squeeze_artifact: str,
                             stage1_artifact: str) -> list[str]:
    """Closure-set invariants (R/S/SM/SG/V/VG): SM reproduces production squeeze;
    S is exactly the production stage-1; SM and SG share that stage-1 and differ
    only in the guard; V/VG keep the fold/grep-only shelf; VG aliases nothing
    guarded; every encoded arm roundtrips."""
    viol = []
    for name, res in arms.items():
        if not res.roundtrip_ok:
            viol.append(f"{name}: roundtrip not byte-exact")
    if arms["SM"].artifact != squeeze_artifact:
        viol.append("SM: does not reproduce production squeeze byte-for-byte")
    if arms["S"].artifact != stage1_artifact:
        viol.append("S: does not equal production stage-1 byte-for-byte")
    # R and V (fold/grep) carry no legend. S MAY carry a structural-codec legend
    # (tmpl/diag alias via templates) — that is the production stage-1, so it is
    # not a violation; the guard is a stage-2 (mine) property only.
    for name in ("R", "V"):
        if legend_of(arms[name].artifact):
            viol.append(f"{name}: alias legend present in an alias-off arm")
    # The GUARD applies to the MINE stage only. So the invariant is that the
    # MINE-ADDED entries (SG.legend − S.legend) never alias a guarded span. A
    # guarded span already aliased by the production stage-1 (tmpl) is untouched
    # by the stage-2 guard — a real finding, not a violation.
    s_legend = set(legend_of(arms["S"].artifact))
    for a, phrase in legend_of(arms["SG"].artifact).items():
        if a not in s_legend and is_guarded_lexical(phrase):
            viol.append(f"SG: MINE aliased a guarded span {a}={phrase!r}")
            break
    for a, phrase in legend_of(arms["VG"].artifact).items():  # VG stage-1 (fold/grep) has no legend
        if is_guarded_lexical(phrase):
            viol.append(f"VG: aliased a guarded span {a}={phrase!r}")
            break
    return viol
