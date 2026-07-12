# LOSS: build-log-rtk-log / n-warnings

- category: **count**  field: `answer`  match: `exact`
- question: How many warnings does this build log report? Put the integer in "answer".
- gold: `['9']`
- source: run `cpu-qwen2.5-coder-7b` commit `0b76e64`  records_sha256 `18e1afcba6f3`
- model `qwen2.5-coder-7b-instruct`  qodec `sha256:07ff3a94830c`  tokenizer `c0382117ea32`

## mechanism: **notation-ambiguity**  (+ structural-folding)
```json
[
  {
    "kind": "gold_count_preserved_verbatim",
    "value": "9",
    "line": "[warn] 9 warnings (3 unique)"
  },
  {
    "kind": "competing_count_present",
    "value": "7",
    "line": "7 Warning(s)"
  },
  {
    "kind": "repetition_grouping_present",
    "lines": [
      "[×5]   CSC : warning CS8618: Non-nullable property 'CurrentUser' must contain a non-null value when ex...",
      "[×3]   warning MSB3277: Found conflicts between different versions of \"DevExpress.Xpf.Core\" that could..."
    ],
    "note": "counted items folded under [×N] markers — grouping evidence, not the source of the wrong count"
  }
]
```

## answers (all arms, all repeats)

| arm | rep | correct | fmt | malformed | leaks | invalid | ptok | ctok | answer_sha |
|-----|-----|---------|-----|-----------|-------|---------|------|------|------------|
| raw | 0 | True | True | False | 0 | 0 | 321 | 30 | d19ccb250b |
| raw | 1 | True | True | False | 0 | 0 | 321 | 30 | d19ccb250b |
| raw | 2 | True | True | False | 0 | 0 | 321 | 30 | d19ccb250b |
| raw+brief | 0 | True | True | False | 0 | 0 | 583 | 30 | d19ccb250b |
| raw+brief | 1 | True | True | False | 0 | 0 | 583 | 30 | d19ccb250b |
| raw+brief | 2 | True | True | False | 0 | 0 | 583 | 30 | d19ccb250b |
| encoded+brief | 0 | False | True | False | 0 | 0 | 582 | 30 | 868a9f9395 |
| encoded+brief | 1 | False | True | False | 0 | 0 | 582 | 30 | 868a9f9395 |
| encoded+brief | 2 | False | True | False | 0 | 0 | 582 | 30 | 868a9f9395 |

## gold span fate
- `9` → **preserved_verbatim**

count checks: {"gold_count": "9", "gold_count_line_in_artifact": "[warn] 9 warnings (3 unique)", "gold_count_preserved_verbatim": true, "competing_count_line": "7 Warning(s)", "competing_count_value": "7", "repetition_markers_present": true, "repetition_marker_lines": ["[×5]   CSC : warning CS8618: Non-nullable property 'CurrentUser' must contain a non-null value when ex...", "[×3]   warning MSB3277: Found conflicts between different versions of \"DevExpress.Xpf.Core\" that could..."]}

## alias dictionary (used)
```
码 =    C:\build\src\Legacy.UI\ViewModels\UserEditorViewModel.cs(1¿): error CS1061: 'UserSession' doe...
```

## raw→encoded diff (+6 / -2), full diff in `build-log-rtk-log__n-warnings.diff`

gold-touching hunks:
```diff
-   C:\build\src\Legacy.UI\ViewModels\UserEditorViewModel.cs(198,22): error CS1061: 'UserSession' doe...
+码|98,22
```
