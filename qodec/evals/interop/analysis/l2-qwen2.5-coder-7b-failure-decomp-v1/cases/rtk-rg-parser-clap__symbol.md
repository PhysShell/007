# CONTROL: rtk-rg-parser-clap / symbol

- category: **locator**  field: `symbols`  match: `one-of`
- question: Name a value-parser type mentioned. Put it in "symbols".
- gold: `['ValueParser']`
- source: run `cpu-qwen2.5-coder-7b` commit `0b76e64`  records_sha256 `18e1afcba6f3`
- model `qwen2.5-coder-7b-instruct`  qodec `sha256:07ff3a94830c`  tokenizer `c0382117ea32`

## answers (all arms, all repeats)

| arm | rep | correct | fmt | malformed | leaks | invalid | ptok | ctok | answer_sha |
|-----|-----|---------|-----|-----------|-------|---------|------|------|------------|
| raw | 0 | True | True | False | 0 | 0 | 977 | 36 | 7e33bbe0ba |
| raw+brief | 0 | True | True | False | 0 | 0 | 1239 | 31 | b06ba4992d |
| encoded+brief | 0 | True | True | False | 0 | 0 | 1011 | 31 | b06ba4992d |

## gold span fate
- `ValueParser` → **preserved_verbatim**

locator checks: [{"full_path": "preserved_verbatim", "basename": "preserved_verbatim", "path_prefix": "not_applicable", "prefix_aliases": []}]

## alias dictionary (used)
```
件 =  `Parser`, whereas now we implement
值 = ://! #
函 =  [`TypedValueParser`][crate::builder::TypedValueParser]
帧 = »src/
引 = ://! A [custom parser][TypedValueParser] can be used to improve the error messages or provide additional validation:
标 = , Subcommand, ValueEnum};
码 = `](https://doc.rust-lang.org/cargo/reference/manifest.html#the-
类 = [`Parser`][crate::Parser]
组 = 值# Configuring the Parser
记 = ://!   
试 = 值 use clap::Parser;
路 = ://! subcommands. The type of the field is usually an enum that derived `Parser`. However, you can
链 = 7://! 
错 = [derive(Parser)]
```

## raw→encoded diff (+57 / -32), full diff in `rtk-rg-parser-clap__symbol.diff`

gold-touching hunks:
```diff
-src/_concepts.rs:100://! the Value will be parsed according to [`ValueParser`]
-src/_concepts.rs:104:use clap_builder::builder::ValueParser;
-src/_cookbook/typed_derive.rs:13://! ## Built-in [`TypedValueParser`][crate::builder::TypedValueParser]
-src/_cookbook/typed_derive.rs:29://! ## Custom [`TypedValueParser`][crate::builder::TypedValueParser]
-src/_tutorial.rs:194://! A [custom parser][TypedValueParser] can be used to improve the error messages or provide additional validation:
-src/_derive/_tutorial.rs:200://! A [custom parser][TypedValueParser] can be used to improve the error messages or provide additional validation:
+引=://! A [custom parser][TypedValueParser] can be used to improve the error messages or provide additional validation:
+函= [`TypedValueParser`][crate::builder::TypedValueParser]
+100://! the Value will be parsed according to [`ValueParser`]
+104:use clap_builder::builder::ValueParser;
```
