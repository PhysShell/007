#!/usr/bin/env python3
"""Negative-recall probe against a deja index built from a synthetic corpus.

Question measured: when the corpus contains no evidence for a question, does
the search ladder stay silent, or does it return a session anyway?
"""
import json, os, pathlib, subprocess, sys, uuid, datetime

ROOT = pathlib.Path("/tmp/claude-0/-home-user-007/cf22df47-424f-55c1-a758-7a9678effd3e/scratchpad/probe")
CLAUDE = ROOT / "claude"
INDEX = ROOT / "index"
DEJA = "/tmp/claude-0/-home-user-007/cf22df47-424f-55c1-a758-7a9678effd3e/scratchpad/deja"

# 24 sessions of real-looking engineering work. Each carries a specific
# technical vocabulary plus the generic words every engineering log shares
# (storage, config, deploy, timeout, runbook, migration, cache, retry).
SESSIONS = [
    ("api-gateway", "postgres connection pool exhausted under load", [
        "the api gateway is timing out, pgbouncer says the pool is exhausted",
        "raised max_connections to 200 and set pool_mode to transaction in the pgbouncer config",
        "the leak was a missing defer rows.Close() in the orders repository, so connections were never returned",
        "added a runbook note: watch pg_stat_activity idle in transaction before touching pool size",
    ]),
    ("api-gateway", "jwt refresh token rotation broke mobile clients", [
        "mobile clients get 401 after the refresh token rotation deploy",
        "the old refresh token was invalidated before the client stored the new one",
        "decision: keep a 60 second grace window where the previous refresh token still validates",
        "added a test that replays the previous token inside the grace window",
    ]),
    ("api-gateway", "rate limiter leaks memory on unknown api keys", [
        "the rate limiter map grows without bound, one entry per unseen api key",
        "switched to an lru with a 10 minute ttl instead of a plain map",
        "memory flat at 300MB after the change, was climbing 1GB a day",
    ]),
    ("api-gateway", "grpc deadline propagation lost across the proxy", [
        "downstream calls keep running after the client gave up",
        "the proxy created a fresh context instead of forwarding the deadline",
        "fixed by threading the incoming context through the handler chain",
    ]),
    ("billing", "stripe webhook retries double charged two customers", [
        "two customers were charged twice after the webhook retry storm",
        "the handler was not idempotent, we keyed on our own id instead of the stripe event id",
        "decision: dedupe on stripe event id in a unique index, reject duplicates at the database",
        "refunded both charges manually and wrote the incident runbook",
    ]),
    ("billing", "invoice pdf generation timeout in the worker", [
        "invoice pdf jobs time out after 30 seconds on large accounts",
        "the renderer was loading every line item into memory before writing",
        "streamed the line items and cut peak memory by 80 percent",
    ]),
    ("billing", "currency rounding mismatch between ledger and invoice", [
        "the ledger total and the invoice total differ by one cent on some orders",
        "we rounded per line item in one place and on the sum in the other",
        "decision: money is integer minor units everywhere, rounding happens once at presentation",
    ]),
    ("billing", "proration logic wrong on mid cycle plan downgrade", [
        "downgrading mid cycle credits the customer too much",
        "the proration used calendar days instead of the billing period length",
        "added property tests over random period lengths",
    ]),
    ("platform", "kubernetes ingress tls certificate not renewing", [
        "cert-manager stopped renewing the ingress certificate, expiry in four days",
        "the http01 solver could not reach the challenge path because of the auth middleware",
        "excluded /.well-known/acme-challenge from the middleware and the order completed",
        "runbook: check certificate and certificaterequest status before touching the issuer",
    ]),
    ("platform", "terraform state lock stuck after a cancelled apply", [
        "terraform plan fails with state blob already locked",
        "a cancelled ci job left the dynamodb lock behind",
        "force-unlock with the lock id from the error, then re-plan to confirm no drift",
    ]),
    ("platform", "node autoscaler thrashing between two and six nodes", [
        "the cluster autoscaler scales up and back down every few minutes",
        "requests were far below actual usage so the scheduler packed pods too tight",
        "set realistic cpu requests from the p95 and the thrashing stopped",
    ]),
    ("platform", "object storage bucket lifecycle deleted needed backups", [
        "the storage lifecycle rule expired objects we still needed for restore",
        "the prefix filter was empty so the rule applied to the whole bucket",
        "restored from versioning, then scoped the rule to the tmp/ prefix and enabled a legal hold on backups",
    ]),
    ("indexer", "rust borrow checker fight in the segment merger", [
        "cannot borrow segments as mutable more than once in the merge loop",
        "split the read phase from the write phase and passed indices instead of references",
        "the merge is now single pass and about 20 percent faster",
    ]),
    ("indexer", "tokenizer drops cyrillic text on the fold path", [
        "russian queries return nothing, the tokenizer treats the text as one term",
        "the wordy rune check assumed anything above a codepoint threshold was a word",
        "fixed with a unicode letter check and added a cyrillic regression test",
    ]),
    ("indexer", "mmap index corruption after a partial write", [
        "the index file is truncated when the process is killed mid write",
        "write to a temp file and rename, rename is atomic on the same filesystem",
        "added a crash test that kills the writer at random offsets",
    ]),
    ("indexer", "bm25 scoring favours very short documents", [
        "one line documents outrank real matches",
        "the length normalisation constant was too aggressive at 0.2",
        "tuned b to 0.75 and re-ran the retrieval regression set",
    ]),
    ("web", "webpack build out of memory in ci", [
        "the ci build dies with javascript heap out of memory",
        "source maps for the vendor bundle were the memory spike",
        "disabled vendor source maps in ci and raised the node heap to 4096",
    ]),
    ("web", "hydration mismatch on the dashboard chart", [
        "react warns about a hydration mismatch on the dashboard chart",
        "the server rendered a locale formatted date, the client rendered another locale",
        "decision: format dates on the client only, server sends the raw timestamp",
    ]),
    ("web", "service worker serving a stale bundle after deploy", [
        "users keep the old bundle after a deploy until a hard refresh",
        "the cache first strategy never revalidated the html entrypoint",
        "switched the entrypoint to network first and kept cache first for hashed assets",
    ]),
    ("data", "protobuf schema migration broke the old consumers", [
        "old consumers cannot parse the new event after the schema migration",
        "a field number was reused for a different type",
        "decision: field numbers are never reused, reserved is mandatory in the review checklist",
    ]),
    ("data", "airflow dag backfill duplicated rows in the warehouse", [
        "the backfill wrote every row twice into the warehouse table",
        "the task was not idempotent, insert instead of merge on the natural key",
        "rewrote as a merge and deduped the affected partitions",
    ]),
    ("data", "parquet column pruning not applied by the reader", [
        "the reader scans every column even when only two are selected",
        "the projection was applied after the read instead of pushed down",
        "pushed the projection into the scan and cut the query from 40s to 6s",
    ]),
    ("data", "clickhouse merge falls behind during the nightly load", [
        "parts pile up during the nightly load and queries slow down",
        "the insert batch size was far too small, thousands of tiny parts per minute",
        "batched inserts to 100k rows and the merge caught up",
    ]),
    ("infra", "ci flake in the integration suite once every twenty runs", [
        "the integration suite fails intermittently on the same test",
        "a fixed port collided when two jobs ran on the same runner",
        "bound to port zero and read the assigned port back",
    ]),
]

# Questions about work that never happened. Each shares generic engineering
# vocabulary with the corpus but names a technology or incident absent from it.
NEGATIVE = [
    "what did we decide about the kafka consumer rebalance storm",
    "how did we fix the elasticsearch shard allocation failure",
    "why did we drop mongodb sharding for the audit log",
    "what was the conclusion on the rabbitmq quorum queue migration",
    "how did we handle the cassandra tombstone compaction runbook",
    "what did we change in the envoy mesh mtls rotation",
    "did we ever solve the android bluetooth pairing timeout",
    "what was the fix for the swift concurrency actor deadlock",
    "how did we tune the hadoop yarn scheduler queues",
    "what did we decide about the graphql federation gateway split",
    "why did the wasm runtime sandbox escape test fail",
    "how did we migrate the ldap directory to okta",
    "what was the outcome of the ipv6 dual stack rollout",
    "did we fix the printer driver spool corruption",
    "how did we configure the snowflake warehouse auto suspend",
    "what did we learn from the dns anycast failover drill",
    "how did we resolve the git lfs storage quota exhaustion",
    "what happened with the arm64 cross compilation toolchain",
    "why did we revert the istio ambient mode upgrade",
    "what did we decide about the firebase push notification retry",
]

# Questions the corpus does answer, as a control that the index is not simply
# returning nothing to everything.
POSITIVE = [
    "why was the postgres connection pool exhausted",
    "how did we fix the stripe webhook double charge",
    "what did we do about the ingress tls certificate renewal",
    "why did the tokenizer drop cyrillic text",
    "what caused the webpack build to run out of memory",
    "how did we handle the protobuf field number reuse",
    "what was the parquet column pruning problem",
    "how did we stop the node autoscaler thrashing",
    "what was the jwt refresh token rotation decision",
    "why did the airflow backfill duplicate rows",
]


def write_corpus():
    base = datetime.datetime(2026, 5, 1, 9, 0, 0, tzinfo=datetime.timezone.utc)
    for n, (project, title, turns) in enumerate(SESSIONS):
        sid = str(uuid.uuid5(uuid.NAMESPACE_DNS, f"deja-probe-{n}"))
        d = CLAUDE / "projects" / f"-home-user-{project}"
        d.mkdir(parents=True, exist_ok=True)
        t = base + datetime.timedelta(days=n)
        lines = []
        text = [title] + turns
        for i, msg in enumerate(text):
            ts = (t + datetime.timedelta(minutes=i)).strftime("%Y-%m-%dT%H:%M:%SZ")
            role = "user" if i % 2 == 0 else "assistant"
            if role == "user":
                lines.append(json.dumps({
                    "type": "user", "sessionId": sid, "cwd": f"/home/user/{project}",
                    "timestamp": ts, "message": {"role": "user", "content": msg}}))
            else:
                lines.append(json.dumps({
                    "type": "assistant", "sessionId": sid, "cwd": f"/home/user/{project}",
                    "timestamp": ts,
                    "message": {"role": "assistant",
                                "content": [{"type": "text", "text": msg}]}}))
        (d / f"{sid}.jsonl").write_text("\n".join(lines) + "\n")


def env():
    e = dict(os.environ)
    e["DEJA_CLAUDE_ROOT"] = str(CLAUDE)
    e["DEJA_INDEX_DIR"] = str(INDEX)
    e["HOME"] = str(ROOT / "home")
    e["XDG_CONFIG_HOME"] = str(ROOT / "home" / ".config")
    e["XDG_DATA_HOME"] = str(ROOT / "home" / ".local" / "share")
    return e


def search(q):
    """Return (hits, tier, dropped) for one query.

    dropped names the query terms deja reported as ignored — a term whose
    variant list is empty matched no session together with the rest. It is the
    signal a caller would need to tell "answered the question" from "answered
    a shorter question it invented".
    """
    p = subprocess.run([DEJA, "--json", q], env=env(), capture_output=True, text=True, timeout=120)
    out = p.stdout.strip()
    if not out:
        return [], "", []
    try:
        data = json.loads(out)
    except json.JSONDecodeError:
        return [], out[:200], []
    if isinstance(data, dict):
        hits = data.get("results") or data.get("sessions") or data.get("hits") or []
        tier = data.get("tier", "")
        variants = data.get("variants") or {}
        dropped = sorted(t for t, v in variants.items() if not [x for x in v if x])
    else:
        hits, tier, dropped = data, "", []
    return hits, tier, dropped


def main():
    write_corpus()
    (ROOT / "home").mkdir(parents=True, exist_ok=True)
    idx = subprocess.run([DEJA, "index", "--rebuild"], env=env(), capture_output=True, text=True, timeout=300)
    print("index:", idx.stdout.strip()[:400] or idx.stderr.strip()[:400])

    report = {"corpus_sessions": len(SESSIONS), "negative": [], "positive": []}
    for label, queries in (("negative", NEGATIVE), ("positive", POSITIVE)):
        for q in queries:
            hits, tier, dropped = search(q)
            first = ""
            if hits:
                h = hits[0]
                if isinstance(h, dict):
                    sess = h.get("session") or {}
                    first = sess.get("title") or h.get("title") or ""
                    tier = tier or h.get("tier", "")
            report[label].append({"query": q, "hits": len(hits), "tier": tier,
                                  "dropped_terms": dropped, "top": first[:90]})

    neg_answered = sum(1 for r in report["negative"] if r["hits"] > 0)
    pos_answered = sum(1 for r in report["positive"] if r["hits"] > 0)
    report["negative_answered"] = neg_answered
    report["negative_total"] = len(NEGATIVE)
    report["positive_answered"] = pos_answered
    report["positive_total"] = len(POSITIVE)
    (ROOT / "report.json").write_text(json.dumps(report, indent=2))

    print(f"\nnegative queries answered with a session: {neg_answered}/{len(NEGATIVE)}")
    print(f"positive control answered:                 {pos_answered}/{len(POSITIVE)}\n")
    for r in report["negative"]:
        print(f"  [{r['hits']:>2} {r['tier']:<9}] {r['query']}\n      -> {r['top']}")
    print()
    for r in report["positive"]:
        print(f"  [{r['hits']:>2} {r['tier']:<9}] {r['query']}\n      -> {r['top']}")


if __name__ == "__main__":
    main()
