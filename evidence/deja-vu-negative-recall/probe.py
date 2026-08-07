#!/usr/bin/env python3
"""Cross-version recall oracle: retrieval drift and admission drift, separated.

Two questions, deliberately not the same question:

  retrieval  — what did the upstream retriever return for this query?
  admission  — what did 007's evidence-admission layer promote to an evidence
               object?

The second one has no implementation yet, so this run reports it as `null`
rather than as zero. A null is an unmeasured slot; a zero would be a claim.
That distinction is the whole point of keeping the two apart: when the
resolver exists, re-running this against the same corpus attributes any change
to one side or the other.

Usage:
    probe.py [--deja PATH] [--work DIR] [--subject-commit SHA]
             [--subject-version TAG]

Writes report.json next to corpus.json.
"""
import argparse, hashlib, json, os, pathlib, re, shutil, subprocess, sys, uuid, datetime

HERE = pathlib.Path(__file__).resolve().parent


def identify_binary(deja, claimed_commit, allow_unverified=False):
    """Bind the report to the binary that produced it, not to operator prose.

    A --subject-commit copied into the report proves nothing on its own: run
    the binary from commit B, claim A, and the artifact lies. So hash the
    binary and read the VCS revision Go stamps into it.

    Once a commit is claimed, every way of *failing* to confirm it is a
    refusal, not a shrug: a mismatch, an absent stamp, and a dirty tree all
    mean the same thing — the report cannot honestly assert the revision it is
    about to print. --allow-unverified-binary is the explicit escape hatch, and
    it is recorded in the report so a reader can see it was used.
    """
    path = shutil.which(deja) or deja
    digest = hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()
    revision, dirty = "", None
    try:
        info = subprocess.run(["go", "version", "-m", path],
                              capture_output=True, text=True, timeout=60).stdout
        m = re.search(r"^\s*build\s+vcs\.revision=(\S+)", info, re.M)
        if m:
            revision = m.group(1)
        m = re.search(r"^\s*build\s+vcs\.modified=(\S+)", info, re.M)
        if m:
            dirty = m.group(1) == "true"
    except (OSError, subprocess.SubprocessError):
        pass  # no Go toolchain here; the hash still binds the artifact

    refusal = None
    if claimed_commit and not revision:
        refusal = ("the binary carries no vcs.revision stamp, so the claim "
                   f"--subject-commit {claimed_commit} cannot be confirmed")
    elif claimed_commit and not revision.startswith(claimed_commit[:12]):
        refusal = (f"--subject-commit {claimed_commit} but the binary is "
                   f"stamped vcs.revision={revision}")
    elif claimed_commit and dirty:
        refusal = (f"the binary was built from a modified tree "
                   f"(vcs.modified=true), so {claimed_commit} does not "
                   "identify the source it ran")
    if refusal and not allow_unverified:
        raise SystemExit(f"refusing to write a report that would lie: {refusal}"
                         "\n(pass --allow-unverified-binary to record it anyway)")

    return {"path": str(path), "sha256": digest, "vcs_revision": revision,
            "vcs_modified": dirty,
            "unverified": bool(refusal), "unverified_reason": refusal}


def build_corpus(corpus, claude_root):
    """Write the corpus as Claude Code JSONL transcripts under claude_root."""
    base = datetime.datetime(2026, 5, 1, 9, 0, 0, tzinfo=datetime.timezone.utc)
    by_title = {}
    for n, s in enumerate(corpus["sessions"]):
        sid = str(uuid.uuid5(uuid.NAMESPACE_DNS, f"deja-probe-{n}"))
        by_title[s["title"]] = sid
        d = claude_root / "projects" / f"-home-user-{s['project']}"
        d.mkdir(parents=True, exist_ok=True)
        t = base + datetime.timedelta(days=n)
        lines = []
        for i, msg in enumerate([s["title"]] + s["turns"]):
            ts = (t + datetime.timedelta(minutes=i)).strftime("%Y-%m-%dT%H:%M:%SZ")
            common = {"sessionId": sid, "cwd": f"/home/user/{s['project']}", "timestamp": ts}
            if i % 2 == 0:
                lines.append(json.dumps({"type": "user", **common,
                                         "message": {"role": "user", "content": msg}}))
            else:
                lines.append(json.dumps({"type": "assistant", **common,
                                         "message": {"role": "assistant",
                                                     "content": [{"type": "text", "text": msg}]}}))
        (d / f"{sid}.jsonl").write_text("\n".join(lines) + "\n")
    return by_title


WORK_MARKER = ".deja-probe-workdir"


def prepare_work(path):
    """Return a wiped workdir this probe is allowed to destroy.

    The wipe is necessary — session ids are derived from the corpus, so a
    bumped corpus id that drops a session would otherwise leave the previous
    corpus's transcripts on disk and quietly test a ghost. But `--work /` would
    then recursively delete /claude, /index and /home, and any shared directory
    loses its same-named children. So only a directory this probe owns gets
    wiped: one that does not exist yet, one that is empty, or one already
    carrying the marker file.
    """
    work = pathlib.Path(path).expanduser().resolve()
    if work == pathlib.Path(work.anchor):
        raise SystemExit(f"--work {work} is a filesystem root; refusing.")
    if work.exists() and not work.is_dir():
        raise SystemExit(f"--work {work} exists and is not a directory; refusing.")
    if work.is_dir() and any(work.iterdir()) and not (work / WORK_MARKER).exists():
        raise SystemExit(
            f"--work {work} is not empty and carries no {WORK_MARKER} marker, so "
            "this probe does not own it and will not delete inside it.\n"
            "Pass a fresh directory, or remove that one yourself first.")

    work.mkdir(parents=True, exist_ok=True)
    (work / WORK_MARKER).write_text(
        "Created by evidence/deja-vu-negative-recall/probe.py.\n"
        "This directory is wiped on every run.\n")
    for sub in ("claude", "index", "home", "tmp"):
        shutil.rmtree(work / sub, ignore_errors=True)
        (work / sub).mkdir(parents=True, exist_ok=True)
    return work


def probe_env(work):
    """Build the child environment from nothing, never from os.environ.

    deja is a third-party binary. Forwarding the ambient environment would hand
    it whatever the operator's shell happens to hold — GITHUB_TOKEN,
    ARLIAI_API_KEY, cloud credentials — and this probe writes a tracked
    report.json out of that process's stdout. The repo's central claim is that
    no credential is in the tree (AGENTS.md rule 1), and the same rule already
    strips ARLIAI_API_KEY from provider subprocess environments; a research
    harness does not get a discount on it.

    Only what deja needs to run and to stay inside the throwaway workdir.
    """
    home = work / "home"
    e = {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),  # deja shells out to sqlite3
        "LC_ALL": "C.UTF-8",                              # deterministic collation
        "HOME": str(home),
        "XDG_CONFIG_HOME": str(home / ".config"),
        "XDG_DATA_HOME": str(home / ".local" / "share"),
        "TMPDIR": str(work / "tmp"),
        "DEJA_CLAUDE_ROOT": str(work / "claude"),
        "DEJA_INDEX_DIR": str(work / "index"),
    }
    if "SystemRoot" in os.environ:  # Windows cannot exec without it
        e["SystemRoot"] = os.environ["SystemRoot"]
    return e


def retrieve(deja, env, q):
    """One query against the upstream retriever.

    dropped names the terms deja reported as ignored — a term whose variant
    list is empty matched no session together with the rest. It is the signal
    an admission layer needs to tell "answered the question" from "answered a
    shorter question it invented".
    """
    p = subprocess.run([deja, "--json", q], env=env, capture_output=True, text=True, timeout=120)
    # A failed query and a query with no matches are different facts, and this
    # harness exists to keep exactly that pair apart. deja exits 0 on an empty
    # result (checked at 7c4a294), so a nonzero status is a harness failure and
    # must never be recorded as a retrieval miss — that would undercount false
    # hits and recall alike while the report still looked complete.
    if p.returncode != 0:
        raise SystemExit(f"deja exited {p.returncode} on query {q!r}; refusing to "
                         f"record it as zero hits.\nstderr: {p.stderr.strip()[:400]}")
    out = p.stdout.strip()
    if not out:
        return {"hits": 0, "tier": "", "dropped_terms": [], "returned_ids": [], "top_title": ""}
    try:
        data = json.loads(out)
    except json.JSONDecodeError:
        raise SystemExit(f"deja returned unparseable JSON on query {q!r}; refusing to "
                         f"record it as a retrieval result.\nstdout: {out[:400]}")
    hits = data.get("hits") or []
    variants = data.get("variants") or {}
    ids, top = [], ""
    for h in hits:
        sess = h.get("session") or {}
        ids.append(sess.get("id", ""))
        if not top:
            top = sess.get("title", "")
    return {
        "hits": len(hits),
        "tier": data.get("tier", ""),
        "dropped_terms": sorted(t for t, v in variants.items() if not [x for x in v if x]),
        "returned_ids": ids,
        "top_title": top,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--deja", default=os.environ.get("DEJA_BIN", "deja"))
    ap.add_argument("--work", default=os.environ.get("DEJA_PROBE_WORK", "/tmp/deja-probe"))
    ap.add_argument("--subject-commit", default=os.environ.get("DEJA_SUBJECT_COMMIT", ""))
    ap.add_argument("--subject-version", default=os.environ.get("DEJA_SUBJECT_VERSION", ""))
    ap.add_argument("--allow-unverified-binary", action="store_true",
                    help="record a report whose subject commit the binary does not confirm")
    args = ap.parse_args()

    corpus = json.loads((HERE / "corpus.json").read_text())
    binary = identify_binary(args.deja, args.subject_commit, args.allow_unverified_binary)
    work = prepare_work(args.work)

    by_title = build_corpus(corpus, work / "claude")
    env = probe_env(work)
    idx = subprocess.run([args.deja, "index", "--rebuild"], env=env,
                         capture_output=True, text=True, timeout=300)
    print("index:", (idx.stdout or idx.stderr).strip()[:300])
    if idx.returncode != 0:
        raise SystemExit(f"deja index --rebuild exited {idx.returncode}; refusing to "
                         f"measure a failed index.\nstderr: {idx.stderr.strip()[:400]}")

    # Exit status alone does not prove the index holds the corpus: at 7c4a294 a
    # deja run against an unusable index directory still exits 0. Without this
    # check a harness failure would serialize as retrieval behaviour — zero hits
    # everywhere, in a revision-bound report that looks complete. That is the
    # exact confusion this whole document is about, so the harness may not
    # commit it.
    listed = subprocess.run([args.deja, "last", "1000", "--json"], env=env,
                            capture_output=True, text=True, timeout=120)
    indexed = -1
    if listed.returncode == 0 and listed.stdout.strip():
        try:
            indexed = len(json.loads(listed.stdout).get("sessions", []))
        except json.JSONDecodeError:
            indexed = -1
    if indexed != len(corpus["sessions"]):
        raise SystemExit(f"index holds {indexed} sessions, corpus has "
                         f"{len(corpus['sessions'])}; refusing to measure it.")

    rows = []
    for q in corpus["queries"]:
        r = retrieve(args.deja, env, q["text"])
        row = {"query_id": q["id"], "oracle": q["oracle"], "query": q["text"], **r}
        if q.get("fixture"):
            row["fixture"] = q["fixture"]
        if q["oracle"] == "supported":
            want = by_title.get(q["evidence_title"], "")
            row["evidence_id"] = want
            row["evidence_returned"] = want in r["returned_ids"]
            row["evidence_at_1"] = bool(r["returned_ids"]) and r["returned_ids"][0] == want
        rows.append(row)

    unsup = [r for r in rows if r["oracle"] == "unsupported"]
    sup = [r for r in rows if r["oracle"] == "supported"]
    report = {
        "schema": 1,
        "corpus": {"id": corpus["id"], "sessions": len(corpus["sessions"]),
                   "queries": len(corpus["queries"])},
        "subject": {"repo": "github.com/vshulcz/deja-vu",
                    "claimed_commit": args.subject_commit,
                    "claimed_version": args.subject_version,
                    "binary_sha256": binary["sha256"],
                    "binary_vcs_revision": binary["vcs_revision"],
                    "binary_vcs_modified": binary["vcs_modified"],
                    "binary_unverified": binary["unverified"],
                    "binary_unverified_reason": binary["unverified_reason"]},
        # What the upstream retriever did. Drifts when deja changes.
        "retrieval": rows,
        # What 007's evidence-admission layer promoted. null = not implemented,
        # which is not the same fact as "zero violations". When a resolver
        # exists, each row is:
        #   {"query_id", "evidence_object_ids": [...], "weak_candidate_ids": [...],
        #    "outcome": "EVIDENCE_AVAILABLE" | "NO_SUPPORTED_EVIDENCE"}
        # with the invariant outcome == EVIDENCE_AVAILABLE iff evidence is
        # non-empty; a violation is an unsupported query where either half fails.
        "admission": None,
        "summary": {
            "unsupported_queries": len(unsup),
            "unsupported_with_retrieval_hits": sum(1 for r in unsup if r["hits"] > 0),
            "supported_queries": len(sup),
            "supported_retrieval_evidence_returned": sum(1 for r in sup if r["evidence_returned"]),
            "supported_retrieval_evidence_at_1": sum(1 for r in sup if r["evidence_at_1"]),
            # The two 007-side numbers. The first is a hard gate, the second an
            # optimization metric; a resolver that always abstains passes the
            # first and scores zero on the second, which is why both are named.
            "unsupported_admission_violations": None,
            "supported_evidence_recall": None,
        },
    }
    (HERE / "report.json").write_text(json.dumps(report, indent=2) + "\n")

    s = report["summary"]
    print(f"\nretrieval — unsupported queries answered: "
          f"{s['unsupported_with_retrieval_hits']}/{s['unsupported_queries']}")
    print(f"retrieval — supported evidence returned:  "
          f"{s['supported_retrieval_evidence_returned']}/{s['supported_queries']}"
          f" (at rank 1: {s['supported_retrieval_evidence_at_1']})")
    print("admission — not implemented (null, not zero)\n")
    for r in rows:
        if r["oracle"] == "unsupported" and r["hits"]:
            mark = f" [{r['fixture']}]" if r.get("fixture") else ""
            print(f"  false hit{mark}: {r['query']}\n"
                  f"    tier={r['tier']} dropped={','.join(r['dropped_terms']) or '-'}\n"
                  f"    -> {r['top_title']}")


if __name__ == "__main__":
    sys.exit(main())
