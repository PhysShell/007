# CONTROL: rg-output-rtk-grep / method

- category: **locator**  field: `symbols`  match: `one-of`
- question: Name a method shown in SessionService.cs. Put it in "symbols".
- gold: `['OpenAsync']`
- source: run `cpu-qwen2.5-coder-7b` commit `0b76e64`  records_sha256 `18e1afcba6f3`
- model `qwen2.5-coder-7b-instruct`  qodec `sha256:07ff3a94830c`  tokenizer `c0382117ea32`

## answers (all arms, all repeats)

| arm | rep | correct | fmt | malformed | leaks | invalid | ptok | ctok | answer_sha |
|-----|-----|---------|-----|-----------|-------|---------|------|------|------------|
| raw | 0 | True | True | False | 0 | 0 | 607 | 35 | 209093ea9f |
| raw+brief | 0 | True | True | False | 0 | 0 | 869 | 35 | 209093ea9f |
| encoded+brief | 0 | True | True | False | 0 | 0 | 818 | 35 | 209093ea9f |

## gold span fate
- `OpenAsync` → **preserved_verbatim**

locator checks: [{"full_path": "preserved_verbatim", "basename": "preserved_verbatim", "path_prefix": "not_applicable", "prefix_aliases": []}]

## alias dictionary (used)
```
值 = Async(CancellationToken
函 = src/Legacy.UI/ViewModels/
引 = , cancellationToken).ConfigureAwait(false);
码 = tests/Legacy.UI.Tests/ViewModels/
类 = src/Legacy.Core/Services/
试 = : private void OnPropertyChanged(string propertyName)
路 = [file]
错 = : public async Task 
```

## raw→encoded diff (+40 / -28), full diff in `rg-output-rtk-grep__method.diff`

gold-touching hunks:
```diff
-    33: public async Task OpenAsync(Credentials credentials, CancellationToken cancellationToken)
+    33错OpenAsync(Credentials credentials, CancellationToken cancellationToken)
-    39: await _sessionService.OpenAsync(credentials, cancellationToken).ConfigureAwait(false);
+    39: await _sessionService.OpenAsync(credentials引
```
