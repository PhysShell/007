# LOSS: clap-derive-explore / top-symbol

- category: **locator**  field: `symbols`  match: `one-of`
- question: Name the value-parser type with the most callers. Put it in "symbols".
- gold: `['ValueParser']`
- source: run `cpu-qwen2.5-coder-7b` commit `0b76e64`  records_sha256 `18e1afcba6f3`
- model `qwen2.5-coder-7b-instruct`  qodec `sha256:07ff3a94830c`  tokenizer `c0382117ea32`

## mechanism: **identifier-or-path-aliasing**  (+ alias-decoding, format-or-integrity)
```json
[
  {
    "kind": "gold_span_represented_only_by_alias",
    "span": "ValueParser",
    "aliases": [
      "错"
    ]
  },
  {
    "kind": "alias_glyph_leaked_in_answer",
    "glyphs": [
      "错"
    ]
  }
]
```

## answers (all arms, all repeats)

| arm | rep | correct | fmt | malformed | leaks | invalid | ptok | ctok | answer_sha |
|-----|-----|---------|-----|-----------|-------|---------|------|------|------------|
| raw | 0 | True | True | False | 0 | 0 | 4511 | 33 | 40e22fc7e0 |
| raw | 1 | True | True | False | 0 | 0 | 4511 | 33 | 40e22fc7e0 |
| raw | 2 | True | True | False | 0 | 0 | 4511 | 33 | 40e22fc7e0 |
| raw+brief | 0 | True | True | False | 0 | 0 | 4773 | 33 | 40e22fc7e0 |
| raw+brief | 1 | True | True | False | 0 | 0 | 4773 | 33 | 40e22fc7e0 |
| raw+brief | 2 | True | True | False | 0 | 0 | 4773 | 33 | 40e22fc7e0 |
| encoded+brief | 0 | False | True | False | 1 | 1 | 4228 | 33 | dc7364881a |
| encoded+brief | 1 | False | True | False | 1 | 1 | 4228 | 33 | dc7364881a |
| encoded+brief | 2 | False | True | False | 1 | 1 | 4228 | 33 | dc7364881a |

## gold span fate
- `ValueParser` → **represented_by_alias**  aliases=['错']

locator checks: [{"full_path": "represented_by_alias", "basename": "represented_by_alias", "path_prefix": "not_applicable", "prefix_aliases": []}]

## alias dictionary (used)
```
串 = 1	    
件 = 	    pub(crate) 
例 = 2	        
值 = 	            "
函 = 	            } else if attr.path().is_ident("
列 = crate::ValueEnum + Clone + Send + Sync +
包 = 4	        
名 = ⚠️ no covering tests found
告 = 库        
图 = /// Parse from `std::env::args_os()`,
块 = 8	    
层 = 错Inner::
帧 = 	            Self::
常 = /// Build a [`Command`] that can
库 = 5	    
建 =  as CommandFactory>::command
异 = _t码DefaultValue
引 = clap_builder/src/builder/
态 = `引value_parser.rs`; 名
数 = f.debug_struct("错::
构 = 0	        
查 = crate::
标 = 	    fn 
树 = 6	        
测 = Some(AttrValue::
点 = 3	        
版 = (method), 
码 = " => Some(MagicAttrName::
类 = Sp::new(AttrKind::
组 = , attr.path().span())
节 = 7	    
行 = value: &std::ffi::OsStr,
表 = 9	    
警 = 表        
记 = 	    // Common enough to optimize
试 = ... (gap) ...
路 = 	            ValueParserInner::
边 = ) -> Result<
链 = #[derive(Copy, Clone,
错 = ValueParser
键 = (引value_parser.rs:
```

## raw→encoded diff (+323 / -278), full diff in `clap-derive-explore__top-symbol.diff`

gold-touching hunks:
```diff
+路=	            ValueParserInner::
+错=ValueParser
-- `ValueParser` (clap_builder/src/builder/value_parser.rs:63) — 8 callers in `clap_builder/src/builder/mod.rs`, `clap_builder/src/builder/action.rs`, `clap_builder/src/builder/arg.rs`, `clap_builder/src/builder/command.rs` +1 more; ⚠️ no covering tests found
-- `MapValueParser` (clap_builder/src/builder/value_parser.rs:2014) — 2 callers in `clap_builder/src/builder/mod.rs`, `clap_builder/src/builder/value_parser.rs`; ⚠️ no covering tests found
-- `AnyValueParser` (clap_builder/src/builder/value_parser.rs:591) — 3 callers in `clap_builder/src/builder/value_parser.rs`; ⚠️ no covering tests found
-91	            "value_parser" => Some(MagicAttrName::ValueParser),
-148	    ValueParser,
-**`clap_builder/src/builder/value_parser.rs`** — calls(calls), ValueParser(struct), ValueParserInner(enum), from(method), references(references), fmt(method), +10 more
-63	pub struct ValueParser(ValueParserInner);
-65	enum ValueParserInner {
-74	    Other(Box<dyn AnyValueParser>),
-77	impl ValueParser {
-566	impl std::fmt::Debug for ValueParser {
-569	            ValueParserInner::Bool => f.debug_struct("ValueParser::bool").finish(),
-570	            ValueParserInner::String => f.debug_struct("ValueParser::string").finish(),
-571	            ValueParserInner::OsString => f.debug_struct("ValueParser::os_string").finish(),
-572	            ValueParserInner::PathBuf => f.debug_struct("ValueParser::path_buf").finish(),
-573	            ValueParserInner::Other(o) => write!(f, "ValueParser::other({:?})", o.type_id()),
-578	impl Clone for ValueParser {
-581	            ValueParserInner::Bool => ValueParserInner::Bool,
-582	            ValueParserInner::String => ValueParserInner::String,
-583	            ValueParserInner::OsString => ValueParserInner::OsString,
-584	            ValueParserInner::PathBuf => ValueParserInner::PathBuf,
-585	            ValueParserInner::Other(o) => ValueParserInner::Other(o.clone_any()),
-590	/// A type-erased wrapper for [`TypedValueParser`].
-591	trait AnyValueParser: Send + Sync + 'static {
-616	    fn clone_any(&self) -> Box<dyn AnyValueParser>;
-619	impl<T, P> AnyValueParser for P
-1079	pub struct EnumValueParser<E: crate::ValueEnum + Clone + Send + Sync + 'static>(
-1083	impl<E: crate::ValueEnum + Clone + Send + Sync + 'static> EnumValueParser<E> {
-1674	/// Useful for composing new [`TypedValueParser`]s
-1677	pub struct BoolValueParser {}
-1679	impl BoolValueParser {
-1680	    /// Implementation for [`ValueParser::bool`]
-2012	/// See [`TypedValueParser::map`]
-2014	pub struct MapValueParser<P, F> {
-2019	impl<P, F, T> MapValueParser<P, F>
```
