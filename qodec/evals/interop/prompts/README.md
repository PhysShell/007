# prompts/ — reader prompt templates (Level 2)

Empty until the reader rung is built. The reader benchmark holds everything
fixed except raw vs qodec, so the prompt is a fixed template per task type. The
model only *reads* the passed output — it does not explore the repo or choose
tools.

Four task types, each answered in one deterministic JSON shape so scoring is
rule-based (never an LLM judging its own vibe):

```json
{ "files": [], "symbols": [], "call_path": [], "facts": [], "answer": "" }
```

1. **fact retrieval** — first error, number of failures, which component causes X.
2. **exact locator** — the precise file, symbol, line, route.
3. **relationship tracing** — a call path or dependency chain.
4. **actionability** — the exact command, path, finding ID, or retrieval handle.

The encoded arm additionally prepends the qodec notation brief (the same text
`qodec probe` / `qodec ab` emit, so the read side never drifts from the codec).
