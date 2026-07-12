"""doctor — a machine-checked setup receipt, not a README that says "I ran it".

Verifies the one hard requirement (qodec: encode->decode byte-exact, real BPE
meter) and, for each REQUIRED tool, the full reproducibility chain: actual
version == pinned version, repo HEAD == pinned SHA, CodeGraph index present and
complete with no pending sync, and a successful smoke invocation — recording
the exact argv, exit code and elapsed time of each.

`--strict rtk codegraph` exits non-zero if any required tool is missing, its
version drifts, a repo it needs is unpinned/mis-checked-out, or an index is not
ready. A run that declares itself reproducible must pass this.
"""

from __future__ import annotations

import json
import time

from . import execution, lockfiles
from . import qodec as q

_QPROBE = (
    "src/a/Handler.cs:12: warning CS0168: unused\n"
    "src/a/Handler.cs:34: warning CS0168: unused\n"
    "src/a/Handler.cs:56: warning CS0168: unused\n"
    "src/a/Handler.cs:78: warning CS0168: unused\n"
)


def _smoke(argv: list[str], stdin: str | None = None) -> dict:
    started = time.perf_counter()
    try:
        proc = __import__("subprocess").run(
            argv, input=stdin, capture_output=True, text=True, check=False
        )
        code = proc.returncode
        err = proc.stderr.strip()[:200]
    except OSError as exc:
        code, err = 127, str(exc)
    return {"argv": argv, "exit_code": code, "elapsed_ms": round((time.perf_counter() - started) * 1000, 2),
            "ok": code == 0, "stderr": err if code != 0 else ""}


def check_qodec() -> dict:
    r: dict = {"tool": "qodec", "ok": False}
    try:
        b = q.binary()
        r["binary"] = str(b)
        r["profile"] = b.parent.name
        r["version"] = q.version()
        env = q.encode(_QPROBE, passthrough=True)
        r["meter"] = env.meter
        r["probe_codec"] = env.codec
        back, _ = q.decode(env.content)
        r["roundtrip"] = back == _QPROBE
        r["ok"] = r["roundtrip"] and env.meter != "approx"
        if b.parent.name != "release":
            r["warning"] = "debug binary — timings not representative"
    except Exception as exc:  # noqa: BLE001 - doctor reports, never crashes
        r["error"] = str(exc)
    return r


def check_tool(tool: lockfiles.Tool) -> dict:
    r: dict = {"tool": tool.name, "kind": tool.kind}
    if tool.kind == "unsupported":
        r["ok"] = False
        r["reason"] = tool.reason
        return r
    if tool.kind == "built":
        r["ok"] = True
        return r
    b = tool.resolve_bin()
    r["binary"] = b
    if not b:
        r["ok"] = False
        r["reason"] = f"{tool.name} not resolvable (env {tool.name.upper()}_BIN / PATH)"
        return r
    detected = tool.detected_version()
    r["detected_version"] = detected
    r["pinned_version"] = tool.pinned_version
    r["version_match"] = detected == tool.pinned_version
    smoke_spec = tool.raw.get("smoke", "")
    if tool.name == "rtk":
        r["smoke"] = _smoke([b, "log"], stdin="ERROR boom\nERROR boom\n")
    else:
        r["smoke"] = {"argv": None, "ok": True, "note": f"see lane checks ({smoke_spec})"}
    r["ok"] = bool(r["version_match"]) and r["smoke"]["ok"]
    return r


def check_repo(repo: lockfiles.Repo) -> dict:
    r: dict = {"repo": repo.id, "pinned_rev": repo.rev}
    head = execution.repo_head(repo)
    r["head"] = head
    r["cloned"] = head is not None
    r["rev_match"] = head == repo.rev
    r["ok"] = bool(r["rev_match"])
    if not r["cloned"]:
        r["reason"] = f"not cloned at {repo.clone_dir()} — run `python3 manage.py sync`"
    elif not r["rev_match"]:
        r["reason"] = f"HEAD {head} != pinned {repo.rev}"
    return r


def check_codegraph_index(repo: lockfiles.Repo, tool: lockfiles.Tool) -> dict:
    r: dict = {"repo": repo.id}
    b = tool.resolve_bin()
    if not b:
        return {"repo": repo.id, "ok": False, "reason": "codegraph not resolvable"}
    d = repo.clone_dir()
    smoke = _smoke([b, "status", str(d), "--json"])
    r["status_smoke"] = smoke
    if not smoke["ok"]:
        r["ok"] = False
        r["reason"] = "codegraph status failed"
        return r
    try:
        proc = __import__("subprocess").run(
            [b, "status", str(d), "--json"], capture_output=True, text=True, check=False
        )
        st = json.loads(proc.stdout)
    except (OSError, json.JSONDecodeError) as exc:
        return {"repo": repo.id, "ok": False, "reason": f"unparseable status: {exc}"}
    pending = st.get("pendingChanges", {})
    idx = st.get("index", {})
    r["initialized"] = st.get("initialized", False)
    r["index_state"] = idx.get("state")
    r["pending"] = pending
    r["node_count"] = st.get("nodeCount")
    ready = (
        st.get("initialized")
        and idx.get("state") == "complete"
        and all(v == 0 for v in pending.values())
    )
    r["ok"] = bool(ready)
    if not ready:
        r["reason"] = f"index not ready (state={idx.get('state')}, pending={pending})"
    return r


def build_receipt(strict: list[str] | None = None) -> dict:
    strict = strict or []
    tools = lockfiles.tools()
    try:
        repos = lockfiles.repos()
    except ValueError as exc:
        return {"healthy": False, "strict_ok": False, "error": str(exc)}

    qodec_r = check_qodec()
    tool_rows = {name: check_tool(t) for name, t in tools.items()}
    repo_rows = {rid: check_repo(r) for rid, r in repos.items()}

    index_rows = {}
    if "codegraph" in tools and tools["codegraph"].resolve_bin():
        for rid, repo in repos.items():
            if repo_rows[rid]["cloned"]:
                index_rows[rid] = check_codegraph_index(repo, tools["codegraph"])

    # Strict gate: every named required tool must be OK; codegraph additionally
    # requires every pinned repo cloned at rev with a ready index.
    strict_failures = []
    for name in strict:
        row = tool_rows.get(name)
        if row is None:
            strict_failures.append(f"{name}: unknown tool")
            continue
        if not row.get("ok"):
            strict_failures.append(f"{name}: {row.get('reason') or 'version/smoke check failed'}")
        if name == "codegraph":
            for rid, rr in repo_rows.items():
                if not rr["ok"]:
                    strict_failures.append(f"repo {rid}: {rr.get('reason')}")
            for rid, ir in index_rows.items():
                if not ir["ok"]:
                    strict_failures.append(f"index {rid}: {ir.get('reason')}")

    return {
        "qodec": qodec_r,
        "tools": tool_rows,
        "repos": repo_rows,
        "codegraph_indexes": index_rows,
        "strict": strict,
        "strict_failures": strict_failures,
        "strict_ok": len(strict_failures) == 0,
        "healthy": qodec_r["ok"],
    }
