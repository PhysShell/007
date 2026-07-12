# CONTROL: build-log-rtk-log / n-errors

- category: **count**  field: `answer`  match: `exact`
- question: How many errors does this build log report? Put the integer in "answer".
- gold: `['3']`
- source: run `cpu-qwen2.5-coder-7b` commit `0b76e64`  records_sha256 `18e1afcba6f3`
- model `qwen2.5-coder-7b-instruct`  qodec `sha256:07ff3a94830c`  tokenizer `c0382117ea32`

## answers (all arms, all repeats)

| arm | rep | correct | fmt | malformed | leaks | invalid | ptok | ctok | answer_sha |
|-----|-----|---------|-----|-----------|-------|---------|------|------|------------|
| raw | 0 | True | True | False | 0 | 0 | 321 | 30 | 024383c403 |
| raw+brief | 0 | True | True | False | 0 | 0 | 583 | 30 | 024383c403 |
| encoded+brief | 0 | True | True | False | 0 | 0 | 582 | 30 | 024383c403 |

## gold span fate
- `3` → **preserved_verbatim**

count checks: {"gold_count": "3", "gold_count_line_in_artifact": "[error] 3 errors (3 unique)", "gold_count_preserved_verbatim": true, "fold_markers_present": true}

## alias dictionary (used)
```
码 =    C:\build\src\Legacy.UI\ViewModels\UserEditorViewModel.cs(1¿): error CS1061: 'UserSession' doe...
```

## raw→encoded diff (+6 / -2), full diff in `build-log-rtk-log__n-errors.diff`

gold-touching hunks:
```diff
    [error] 3 errors (3 unique)
-   C:\build\src\Legacy.UI\ViewModels\UserEditorViewModel.cs(142,38): error CS1061: 'UserSession' doe...
+码|42,38
@@ -13,2 +16,3 @@
    [×3]   warning MSB3277: Found conflicts between different versions of "DevExpress.Xpf.Core" that could...
```
