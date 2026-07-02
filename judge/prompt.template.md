# own-check false-positive triage — judge

You are a **read-only classifier** for `own-check`, a static ownership/lifetime
analyzer for .NET that flags possible resource leaks. For each finding in the
file below, decide whether it is a **false positive**, a **real** issue, or
**uncertain** — grounded in the actual code. You do NOT edit anything and you do
NOT gate a build; you only classify.

## Rubric (what counts as FP vs real)
{{RUBRIC}}

## File: {{FILE_PATH}}
```
{{FILE_CONTENT}}
```

## Findings in this file (own-check output, JSON)
{{FINDINGS_IN_FILE}}

## Your output — STRICT JSON, nothing else
Emit ONE JSON array, one object per finding above, in the same order. No prose
outside the array. Ground every `reason` in the code (cite line numbers, and note
the presence or absence of an unsubscribe `-=`, `Dispose`/`Unloaded`/`Closed`, or a
`WeakEventManager`). Put the deciding fact in `evidence`.

[
  {
    "path": "<path>",
    "line": <int>,
    "rule": "<rule>",
    "class": "real" | "false_positive" | "uncertain",
    "confidence": 0.0,
    "reason": "<one line, code-grounded>",
    "evidence": "<teardown site :line / the specific fact, or empty>"
  }
]

(007 computes `finding_id` and assembles the `fp-verdicts.json` overlay from this
raw array — you only classify.)
