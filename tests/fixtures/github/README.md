# GitHub/API classifier fixtures (issue #147, Step 0B)

**Kind: high-fidelity reconstructed API-shaped fixtures. NOT live captures.**

These are hand-authored JSON documents shaped like GitHub API responses. They are
**not** byte-preserved HTTP responses, and nothing here may be cited as "what
GitHub actually returned". They were reconstructed from evidence recorded during
the #145 review rounds and the #147 design work.

Calling them *full-fidelity* would be an evidentiary claim stronger than the
data supports, and a later reader would reasonably take it to mean a real
capture. The honest label is *reconstructed*.

## Rules these files follow

1. **The JSON stays pure API-shaped.** No `_comment`, `_fixture_metadata` or
   `_decisive` keys inside any payload. A classifier tested against an invented
   schema is green about an entity that does not exist in nature. All
   commentary lives in this file instead.
2. **Not minimized to what a classifier reads today.** The complete
   closure-relevant entity is preserved even where no consumer reads it yet —
   `submitted_at`, `pull_request_review_id`, `in_reply_to_id`,
   `original_commit_id`, `author_association`, per-check `head_sha`. Which field
   turns out to be decisive is exactly what this project has repeatedly
   discovered late.
3. **Completeness is bounded.** Avatar URLs, gravatar ids, node ids,
   permissions, reactions, label and milestone flora are omitted: they carry no
   identity, chronology or review semantics for these three conditions. The
   omission is deliberate, not an oversight — see *Known limits*.
4. **Observed condition, never expected output.** Each entry below records what
   is observable in the document. It does **not** record what a classifier
   should decide. Writing the verdict here would make the fixture an answer key
   and the later test a confirmation of its own crib sheet.

## Envelope convention

Each file is an object whose keys name the API surface that produced the value.
The envelope exists only to carry several surfaces in one document; the values
are the API shapes.

| key | surface |
|---|---|
| `pull_request` | `GET /repos/{owner}/{repo}/pulls/{number}` |
| `reviews` | `GET /repos/{owner}/{repo}/pulls/{number}/reviews` |
| `review_comments` | `GET /repos/{owner}/{repo}/pulls/{number}/comments` |
| `issue_comments` | `GET /repos/{owner}/{repo}/issues/{number}/comments` |
| `check_runs` | `GET /repos/{owner}/{repo}/commits/{sha}/check-runs` |

---

## `stale-review-wrong-sha.json`

```text
Kind: reconstructed API fixture
Represents: a submitted review whose reviewed commit differs from the
            pull request head under evaluation

Observed condition:
  /reviews/0/commit_id  !=  /pull_request/head/sha

Decisive observations:
  /pull_request/head/sha
  /reviews/0/commit_id
  /reviews/0/submitted_at
  /review_comments/0/commit_id
  /check_runs/check_runs/0/head_sha

Not a live capture.
Reconstructed from evidence recorded during #145.
Fields outside the relevant review/check/pull-request surfaces are
intentionally omitted.
```

The check runs are bound to the head, the review is bound to a superseded
commit. Both surfaces are individually well-formed.

## `falsification-in-comment.json`

```text
Kind: reconstructed API fixture
Represents: a concrete, reproducible defect claim present on a surface that
            carries no verdict, while the review surface at the same head
            records no finding

Observed condition:
  /issue_comments/0 contains a defect claim with a reproduction
  /reviews/0/commit_id == /pull_request/head/sha
  /review_comments is empty

Decisive observations:
  /pull_request/head/sha
  /reviews/0/commit_id
  /reviews/0/body
  /issue_comments/0/body
  /issue_comments/0/user/login

Not a live capture.
Reconstructed from evidence recorded during #145.
Fields outside the relevant review/comment/pull-request surfaces are
intentionally omitted.
```

The two surfaces disagree about whether anything is wrong, and the surface
carrying the checkable claim is the one that cannot carry a verdict.

## `conflicting-review-surfaces.json`

```text
Kind: reconstructed API fixture
Represents: one vendor, one head, two surfaces bearing mutually incompatible
            closure-relevant observations

Observed condition:
  /reviews/0/commit_id == /pull_request/head/sha
  /review_comments/0 records a defect at that same commit
  /issue_comments/0 states that the same commit was reviewed with no issues
  /reviews/0/user/login == /issue_comments/0/user/login

Decisive observations:
  /pull_request/head/sha
  /reviews/0/commit_id
  /reviews/0/user/login
  /review_comments/0/commit_id
  /review_comments/0/pull_request_review_id
  /issue_comments/0/user/login
  /issue_comments/0/body
  /issue_comments/0/created_at

Not a live capture.
Reconstructed from evidence recorded during #145.
Fields outside the relevant review/comment/pull-request surfaces are
intentionally omitted.
```

Same bot, same SHA, opposite content. Nothing here says which surface wins.

---

## Provenance detail

Real, taken from #145's public history:

- all commit SHAs, and the pull request / base / head refs;
- `check_runs[*].id`, names, and timestamps;
- `review_comments[*].id` — `3791514579` and `3791453710` — and their paths,
  lines and finding text (abridged in the body field);
- bot identities and numeric user ids.

Synthesized, and marked so rather than passed off:

- `reviews[*].id` and `issue_comments[*].id`, and the `html_url` values derived
  from them. The review-object ids were not recorded at the time.
- The **combination** in `stale-review-wrong-sha.json`: a real review at
  `c08abce…` is presented against a later real head `ed7969c…`. Both existed;
  this pairing did not occur in that exact form.
- The clean issue comment in `conflicting-review-surfaces.json`. No such comment
  was posted; it is the counterfactual the condition requires.

## Known limits

- These are reconstructions. Field sets, ordering and absent keys reflect a
  reading of the API, not a recording of it. A real capture would supersede any
  of them.
- Pagination is not represented: each surface appears as a single complete
  array. A classifier that must page cannot be exercised against these three
  as they stand.
- Reaction surfaces are absent, so the "a reaction is not a verdict" rule has
  no specimen here. That rule is stated in #147 and remains without a fixture.
- No rate-limit or API-error specimen is included, so the
  `OWED` versus `CANNOT_CHECK` distinction is likewise unwitnessed at this
  layer.

Those last two are gaps in this set, recorded rather than left to be noticed
later.
