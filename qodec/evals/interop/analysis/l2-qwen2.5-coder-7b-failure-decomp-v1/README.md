# Level-2 failure decomposition — five stable codec losses

Offline decomposition of the canonical run `cpu-qwen2.5-coder-7b`. No model/qodec/network; source verified against its own SHA256SUMS.

**Conclusion: evidence suggests fold × alias interaction**

This decomposes *why* the five stable codec losses happen; it does not revisit the canonical verdict (general comprehension drop) and does not propose protected spans.

## losses

| case | question | category | primary mechanism | secondary |
|------|----------|----------|-------------------|-----------|
| build-log-rtk-log | n-warnings | count | **notation-ambiguity** | structural-folding |
| clap-derive-explore | def-path | locator | **identifier-or-path-aliasing** | alias-decoding |
| clap-derive-explore | top-symbol | locator | **identifier-or-path-aliasing** | alias-decoding, format-or-integrity |
| rtk-rg-derive-clap | file | locator | **mixed** | identifier-or-path-aliasing, grouping-or-boundary-loss, alias-decoding |
| rtk-rg-parser-clap | file | locator | **grouping-or-boundary-loss** | identifier-or-path-aliasing |

## matched controls (deterministic selection)

| loss | control | same cat | same case | Δtokens | Δalias# | Δdensity |
|------|---------|----------|-----------|---------|---------|----------|
| build-log-rtk-log/n-warnings | build-log-rtk-log/n-errors | True | True | 0 | 0 | 0.0 |
| clap-derive-explore/def-path | clap-derive-explore/trait | True | True | 0 | 0 | 0.0 |
| clap-derive-explore/top-symbol | clap-derive-explore/trait-path | True | True | 6 | 0 | 0.0 |
| rtk-rg-derive-clap/file | rtk-rg-parser-clap/symbol | True | False | 1304 | 9 | 6.1369 |
| rtk-rg-parser-clap/file | rg-output-rtk-grep/method | True | False | 198 | 6 | 4.3759 |

## summary
```json
{
  "source_run": "cpu-qwen2.5-coder-7b",
  "n_losses": 5,
  "losses_by_category": {
    "facts/counts": 1,
    "locator": 4
  },
  "losses_by_primary_mechanism": {
    "notation-ambiguity": 1,
    "identifier-or-path-aliasing": 2,
    "mixed": 1,
    "grouping-or-boundary-loss": 1
  },
  "mechanisms_in_facts_counts": [
    "notation-ambiguity"
  ],
  "mechanisms_in_locator": [
    "grouping-or-boundary-loss",
    "identifier-or-path-aliasing",
    "mixed"
  ],
  "alias_losses_vs_controls": {
    "losses": {
      "alias_count_mean": 24.0,
      "alias_density_mean": 5.3789
    },
    "controls": {
      "alias_count_mean": 21.0,
      "alias_density_mean": 5.7311
    }
  },
  "encoded_tokens_losses_vs_controls": {
    "losses": 2474.2,
    "controls": 2175.0
  },
  "gold_span_share_losses": {
    "total_gold_spans": 5,
    "verbatim": 2,
    "alias_only": 3,
    "structurally_rewritten": 0,
    "absent": 0
  },
  "gold_span_share_controls": {
    "total_gold_spans": 5,
    "verbatim": 5,
    "alias_only": 0,
    "structurally_rewritten": 0,
    "absent": 0
  },
  "integrity": {
    "losses_with_alias_leaks": 1,
    "losses_with_invalid_identifiers": 3,
    "losses_with_malformed_encoded": 0
  },
  "conclusion": "evidence suggests fold × alias interaction"
}
```

Files: `summary.json`, `losses.json`, `controls.json`, `cases/`. Integrity: `SHA256SUMS`.
