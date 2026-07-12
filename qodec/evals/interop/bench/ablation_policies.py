"""Factorized alias × structural ablation policies (eval-only).

Six arms decompose the production `squeeze` codec into its factors so a targeted
run can attribute a codec loss to aliasing, structural folding, their
interaction, the `%q1` framing, or a generic lexical guard:

    R   raw+brief                      alias=off structural=off  (no container)
    I   %q1 identity container         alias=off structural=off  (framing only)
    M   mine                           alias=on  structural=off
    F   structural (fold/grep)         alias=off structural=on
    MF  squeeze  (== production)        alias=on  structural=on
    GF  squeeze-guarded                alias=on  structural=on  lexical_guard=on

Production `squeeze` is untouched; MF calls it and must reproduce it byte-for-byte.
Each arm drives the built qodec binary (no reimplementation), records a stage
receipt, and is checked for byte-exact roundtrip. GF's guard is generic and
surface-only (no task/gold) — a diagnostic, NOT protected spans.
"""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
from dataclasses import dataclass

# Arms as (name, codec, alias, structural, guard). R has no codec.
POLICIES = [
    ("R", None, False, False, False),
    ("I", "identity", False, False, False),
    ("M", "mine", True, False, False),
    ("F", "structural", False, True, False),
    ("MF", "squeeze", True, True, False),
    ("GF", "squeeze-guarded", True, True, True),
]
ARM_NAMES = [p[0] for p in POLICIES]


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


def encode_all_arms(raw_payload: str, meter: str, qodec_bin: str) -> dict:
    return {p[0]: apply_policy(p, raw_payload, meter, qodec_bin) for p in POLICIES}


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
    # GF never aliases a guarded span.
    for a, phrase in legend_of(arms["GF"].artifact).items():
        if is_guarded_lexical(phrase):
            viol.append(f"GF: aliased a guarded span {a}={phrase!r}")
            break
    # MF reproduces production squeeze byte-for-byte.
    if squeeze_artifact is not None and arms["MF"].artifact != squeeze_artifact:
        viol.append("MF: does not reproduce production squeeze byte-for-byte")
    # I is a real container (framing applied), not a passthrough.
    if not arms["I"].artifact.startswith("%q1 identity"):
        viol.append("I: not an identity container")
    return viol
