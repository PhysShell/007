#!/usr/bin/env python3
"""Non-scoring RTK×qodec smoke runner.

NON-BENCHMARK · NON-GATING · NOT part of the 48 base cases · NOT held-out.

Proves the plumbing with REAL, pinned RTK integration:
  * each fixture runs a real `rtk pipe` subcommand (a declared `--filter` or an
    explicit `--passthrough`), never a bare no-op invocation;
  * RTK must exit 0; a nonzero exit is a smoke failure;
  * required RTK stdout must be non-empty;
  * qodec is verified lossless over the ACTUAL RTK stdout;
  * hybrid tokens <= actual RTK-stdout tokens (target tokenizer, no chars/4);
  * every execution is recorded with argv, exit code and stream digests;
  * a full reproducibility-identity block is assembled and the run FAILS if any
    mandatory identity field is missing.

RTK output is NOT required to be smaller than raw: RTK's `never_worse` guard may
legitimately return the raw input, which is recorded as a `passthrough`.

It runs no model. The report is written to an output directory (never committed).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
FIXTURES = HERE / "fixtures"
CODEC = "fold-grep-guarded"  # the frozen VG policy codec
RTK_PIN = "5d32d0736f686b69d1e8b9dc45c007d4eb77a0a2"

# Canonical environment exported to child processes (reproducibility).
CANONICAL_ENV = {"LC_ALL": "C.UTF-8", "LANG": "C.UTF-8", "TZ": "UTC"}
ENV_ALLOWLIST = sorted([
    "PATH", "HOME", "LC_ALL", "LANG", "TZ",
    "QODEC_BIN", "RTK_BIN", "SMOKE_OUT",
    "NIX_VERSION", "NIX_SYSTEM", "NIXPKGS_REV", "REPO_COMMIT_SHA",
    "QODEC_TREE_SHA", "FLAKE_LOCK_SHA256", "RUST_TOOLCHAIN_IDENTITY",
    "RTK_SOURCE_SHA",
])
TOKENIZER_PROBES = ["", "hello world\n", "error[E0308]: mismatched types\n",
                    "src/core/parse.rs:120:17\n", "运行 6 tests\n", "\t\r\n{}"]
MANDATORY_IDENTITY = [
    "flake_lock_sha256", "nix_system", "nix_version", "nixpkgs_revision",
    "rust_toolchain_identity", "qodec_source_sha", "qodec_binary_sha256",
    "rtk_source_sha", "rtk_binary_sha256", "locale", "timezone",
    "environment_variable_allowlist", "tokenizer_identity", "tokenizer_sha256",
]


def sha256_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def sha256_file(p: Path) -> str | None:
    return sha256_bytes(p.read_bytes()) if p.exists() else None


def _child_env() -> dict:
    env = dict(CANONICAL_ENV)
    env["PATH"] = os.environ.get("PATH", "/usr/bin:/bin")
    env["HOME"] = os.environ.get("SMOKE_HOME") or os.environ.get("HOME") or "/tmp"
    return env


def run(cmd: list[str], stdin: bytes) -> dict:
    t0 = time.monotonic()
    p = subprocess.run(cmd, input=stdin, stdout=subprocess.PIPE,
                       stderr=subprocess.PIPE, env=_child_env())
    wall = time.monotonic() - t0
    return {
        "command": cmd, "cwd": os.getcwd(),
        "stdin_sha256": sha256_bytes(stdin),
        "stdout_sha256": sha256_bytes(p.stdout),
        "stderr_sha256": sha256_bytes(p.stderr),
        "exit_code": p.returncode, "wall_time_s": round(wall, 6),
        "stdout": p.stdout, "stderr": p.stderr,
    }


def _strip(rec: dict | None) -> dict | None:
    if rec is None:
        return None
    return {k: v for k, v in rec.items() if k not in ("stdout", "stderr")}


def qodec_envelope(qodec_bin: str, text: bytes, meter: str) -> tuple[dict, dict]:
    cmd = [qodec_bin, "encode", "--codec", CODEC, "--meter", meter,
           "--passthrough-on-no-gain", "--json"]
    rec = run(cmd, text)
    if rec["exit_code"] != 0:
        raise RuntimeError(f"qodec encode failed: {rec['stderr'][:400]!r}")
    return json.loads(rec["stdout"].decode("utf-8").strip()), rec


def qodec_decode(qodec_bin: str, content: str) -> bytes:
    rec = run([qodec_bin, "decode"], content.encode("utf-8"))
    if rec["exit_code"] != 0:
        raise RuntimeError(f"qodec decode failed: {rec['stderr'][:400]!r}")
    return rec["stdout"]


def token_count(qodec_bin: str, text: bytes, meter: str) -> int:
    env, _ = qodec_envelope(qodec_bin, text, meter)
    return int(env["tokens_in"])


def qodec_arm(qodec_bin: str, raw: bytes, meter: str) -> dict:
    env, enc_rec = qodec_envelope(qodec_bin, raw, meter)
    content = env["content"]
    decoded = qodec_decode(qodec_bin, content) if env["encoded"] else content.encode("utf-8")
    return {
        "tokens_in": int(env["tokens_in"]), "tokens_out": int(env["tokens_out"]),
        "codec": env["codec"], "encoded": env["encoded"],
        "roundtrip_ok": decoded == raw, "encode_receipt": _strip(enc_rec),
    }


def rtk_pipe(rtk_bin: str, raw: bytes, mode: str, flt: str | None) -> tuple[bytes, dict, list[str]]:
    """Invoke a real `rtk pipe` subcommand for one fixture."""
    if mode == "passthrough":
        argv = [rtk_bin, "pipe", "--passthrough"]
    elif mode == "pipe-auto":
        argv = [rtk_bin, "pipe"]
    elif mode == "pipe-filter":
        argv = [rtk_bin, "pipe", "--filter", flt]
    else:
        raise ValueError(f"unknown rtk_mode {mode!r}")
    rec = run(argv, raw)
    return rec["stdout"], rec, argv


def tokenizer_fingerprint(qodec_bin: str, meter: str) -> str:
    counts = [token_count(qodec_bin, p.encode("utf-8"), meter) for p in TOKENIZER_PROBES]
    return sha256_bytes(json.dumps({"meter": meter, "probe_tokens": counts}, sort_keys=True).encode())


def _git(root: Path, *args: str) -> str | None:
    try:
        r = subprocess.run(["git", "-C", str(root), *args],
                           capture_output=True, text=True)
        return r.stdout.strip() if r.returncode == 0 and r.stdout.strip() else None
    except Exception:
        return None


def find_repo_root() -> Path | None:
    for base in (HERE, Path.cwd()):
        cur = base
        for _ in range(8):
            if (cur / "flake.lock").exists():
                return cur
            if cur.parent == cur:
                break
            cur = cur.parent
    return None


def assemble_identity(qodec_bin: str, rtk_bin: str, meter: str,
                      rtk_source_sha: str) -> dict:
    root = find_repo_root()
    lock = root / "flake.lock" if root else None
    nixpkgs_rev = os.environ.get("NIXPKGS_REV")
    rust_ident = os.environ.get("RUST_TOOLCHAIN_IDENTITY")
    if root:
        if not nixpkgs_rev and lock and lock.exists():
            try:
                d = json.loads(lock.read_text())
                nixpkgs_rev = d["nodes"]["nixpkgs_2"]["locked"]["rev"]
            except Exception:
                pass
        if not rust_ident and (root / "rust-toolchain.toml").exists():
            rust_ident = sha256_file(root / "rust-toolchain.toml")
    repo_commit = os.environ.get("REPO_COMMIT_SHA") or (_git(root, "rev-parse", "HEAD") if root else None)
    qodec_tree = os.environ.get("QODEC_TREE_SHA") or (_git(root, "rev-parse", "HEAD:qodec") if root else None)
    qodec_source_sha = None
    if repo_commit:
        qodec_source_sha = f"repo:{repo_commit}" + (f"+qodec-tree:{qodec_tree}" if qodec_tree else "")
    flake_lock_sha = os.environ.get("FLAKE_LOCK_SHA256") or (sha256_file(lock) if lock else None)
    nix_system = os.environ.get("NIX_SYSTEM") or f"{platform.machine()}-{platform.system().lower()}"

    return {
        "flake_lock_sha256": flake_lock_sha,
        "nix_system": nix_system,
        "nix_version": os.environ.get("NIX_VERSION"),
        "nixpkgs_revision": nixpkgs_rev,
        "rust_toolchain_identity": rust_ident,
        "repository_commit_sha": repo_commit,
        "qodec_tree_sha": qodec_tree,
        "qodec_source_sha": qodec_source_sha,
        "qodec_binary_sha256": sha256_file(Path(qodec_bin)),
        "rtk_source_sha": rtk_source_sha,
        "rtk_binary_sha256": sha256_file(Path(rtk_bin)),
        "locale": CANONICAL_ENV["LC_ALL"],
        "timezone": CANONICAL_ENV["TZ"],
        "environment_variable_allowlist": ENV_ALLOWLIST,
        "tokenizer_identity": meter,
        "tokenizer_sha256": tokenizer_fingerprint(qodec_bin, meter),
    }


def load_fixture_manifest() -> list[dict]:
    return json.loads((FIXTURES / "manifest.json").read_text())


def smoke(qodec_bin: str, rtk_bin: str, meter: str, rtk_source_sha: str) -> dict:
    identity = assemble_identity(qodec_bin, rtk_bin, meter, rtk_source_sha)
    invariants = []

    def check(name: str, ok: bool, detail: str = ""):
        invariants.append({"invariant": name, "ok": bool(ok), "detail": detail})

    # identity gate — fail early if any mandatory field is absent
    missing = [k for k in MANDATORY_IDENTITY if not identity.get(k)]
    check("all mandatory identity fields populated", not missing,
          f"missing: {missing}" if missing else "")

    results = []
    for spec in load_fixture_manifest():
        name = spec["fixture"]
        mode = spec["rtk_mode"]
        flt = spec.get("rtk_filter")
        raw = (FIXTURES / name).read_bytes()
        entry = {"fixture": name, "raw_sha256": sha256_bytes(raw),
                 "declared_mode": mode, "declared_filter": flt}

        raw_tokens = token_count(qodec_bin, raw, meter)
        entry["raw"] = {"tokens": raw_tokens}

        # qodec-over-raw arm
        q = qodec_arm(qodec_bin, raw, meter)
        entry["qodec"] = q
        check(f"decode(qodec(raw))==raw [{name}]", q["roundtrip_ok"])
        check(f"qodec_tokens<=raw_tokens [{name}]", q["tokens_out"] <= raw_tokens,
              f"{q['tokens_out']} <= {raw_tokens}")

        # real RTK arm
        reduced, rtk_rec, argv = rtk_pipe(rtk_bin, raw, mode, flt)
        changed = reduced != raw
        classification = "reduced" if changed else "passthrough"
        support = "unsupported-explicit-passthrough" if mode == "passthrough" else "supported"
        entry["rtk"] = {
            "argv": argv, "exit_code": rtk_rec["exit_code"],
            "stdout_sha256": rtk_rec["stdout_sha256"], "stderr_sha256": rtk_rec["stderr_sha256"],
            "stderr_len": len(rtk_rec["stderr"]), "stdout_len": len(reduced),
            "stdout_empty": len(reduced) == 0, "changed": changed,
            "never_worse_returned_raw": (not changed) and mode != "passthrough",
            "classification": classification, "support": support,
            "receipt": _strip(rtk_rec),
        }
        check(f"rtk execution succeeded [{name}]", rtk_rec["exit_code"] == 0,
              f"exit={rtk_rec['exit_code']}")
        # required RTK output must be non-empty (all fixtures are non-empty)
        check(f"rtk stdout non-empty [{name}]", len(reduced) > 0)

        rtk_tokens = token_count(qodec_bin, reduced, meter)
        entry["rtk"]["tokens"] = rtk_tokens

        # hybrid: qodec over ACTUAL rtk stdout
        h = qodec_arm(qodec_bin, reduced, meter)
        entry["rtk+qodec"] = h
        check(f"decode(qodec(rtk(raw)))==rtk(raw) [{name}]", h["roundtrip_ok"])
        check(f"hybrid_tokens<=rtk_tokens [{name}]", h["tokens_out"] <= rtk_tokens,
              f"{h['tokens_out']} <= {rtk_tokens}")

        results.append(entry)

    report = {
        "kind": "NON-BENCHMARK-SMOKE", "gating": False,
        "meter": meter, "codec": CODEC,
        "identity": identity, "results": results, "invariants": invariants,
        "all_invariants_ok": all(i["ok"] for i in invariants),
    }
    return report


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Real-RTK non-scoring smoke runner.")
    ap.add_argument("--qodec", default=os.environ.get("QODEC_BIN", "qodec"))
    ap.add_argument("--rtk", default=os.environ.get("RTK_BIN"))
    ap.add_argument("--meter", default="o200k")
    ap.add_argument("--out", default=os.environ.get("SMOKE_OUT", "smoke-out"))
    ap.add_argument("--rtk-source-sha",
                    default=os.environ.get("RTK_SOURCE_SHA", RTK_PIN))
    args = ap.parse_args(argv)

    if not args.rtk or not Path(args.rtk).exists():
        print("SMOKE FAILED: a pinned RTK binary is required (--rtk / $RTK_BIN). "
              "This suite performs REAL RTK integration, not a stub.", file=sys.stderr)
        return 2

    report = smoke(args.qodec, args.rtk, args.meter, args.rtk_source_sha)

    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "smoke-report.json").write_text(json.dumps(report, indent=2, sort_keys=True))

    for inv in report["invariants"]:
        print(f"[{'ok ' if inv['ok'] else 'FAIL'}] {inv['invariant']} {inv['detail']}")
    print(f"\nreport -> {out_dir / 'smoke-report.json'}")
    print("ALL INVARIANTS OK" if report["all_invariants_ok"] else "SMOKE FAILED")
    return 0 if report["all_invariants_ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
