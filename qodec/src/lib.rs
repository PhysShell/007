//! qodec — Q's codec lab: token-aware lossless encode/decode experiments for
//! agent context. See `docs/token-codec.md` in the repo root for the design
//! record and measured results.

pub mod ab;
pub mod adapter;
pub mod alias;
pub mod bench;
pub mod container;
pub mod diag;
pub mod fold;
pub mod grep;
pub mod legend;
pub mod meter;
pub mod mine;
pub mod mosaic;
pub mod ppl;
pub mod profile;
pub mod rank;
pub mod rules;
pub mod sam;
pub mod slice;
pub mod tmpl;
pub mod toon;

use anyhow::{bail, Result};

use crate::alias::Alphabet;
use crate::meter::TokenMeter;
use crate::mine::{MineOptions, MinerKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecKind {
    Mine,
    /// `mine` with suffix-automaton candidate discovery: every repeated
    /// substring, any boundary. Same container, same decode.
    Deep,
    Fold,
    Toon,
    /// Group `path:line[:col]:text` matcher output by file path — the
    /// `rg --heading` shape, byte roundtrip.
    Grep,
    /// Template mining for diagnostic streams (`path:line: warning: …` /
    /// MSBuild): repeated tails go to the legend once, quoted identifiers
    /// become slot values. Byte roundtrip, one linear pass.
    Diag,
    /// Drain-style template mining for arbitrary line-based logs: lines
    /// cluster by skeleton, varying positions become slots. No format
    /// rules needed. Byte roundtrip.
    Tmpl,
    /// Pipeline: `toon` (JSON) or the measured best of
    /// `fold`/`grep`/`diag`/`tmpl` (text), then the better of the two
    /// miners over the result.
    Squeeze,
    /// Measured optimal segmentation: cut the payload at line boundaries and
    /// route each region to the cheapest structural codec (shortest path over
    /// span candidates), then mine the whole assembled artifact. The
    /// orchestration layer above the specialized codecs. See `mosaic.rs`.
    Mosaic,
    /// Eval-only. A `%q1 identity` container: byte-identical body, no alias, no
    /// structural transform. Isolates the `%q1` framing itself (the ablation I
    /// arm). alias=off, structural=off.
    Identity,
    /// Eval-only. Structural folding/grouping ONLY — the measured cheaper of
    /// `fold`/`grep`, with full verbatim paths and no glyph aliases (the F arm).
    /// alias=off, structural=on.
    Structural,
    /// Eval-only. The VERBATIM structural shelf (fold/grep only — NOT the full
    /// production squeeze shelf of toon/diag/tmpl) followed by a guarded mine
    /// that never aliases paths / code spans / `::`,snake,Camel identifiers /
    /// grep markers. This is the ablation VG arm; it is NOT "guarded squeeze",
    /// because it also drops diag/tmpl/toon. Production `squeeze` is untouched.
    FoldGrepGuarded,
    /// Eval-only. Production squeeze's stage 1 ONLY (`squeeze_stage1`), no mine —
    /// the S arm. Isolates the production structural stage from the mining.
    SqueezeStage1,
    /// Eval-only. Production stage 1 + a GUARDED mine/deep — the SG arm, the true
    /// "guarded squeeze". It shares the exact stage-1 artifact with `squeeze`
    /// (== SM), so SM and SG differ ONLY in the mine's lexical guard.
    SqueezeMineGuarded,
}

impl CodecKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mine" => Some(Self::Mine),
            "deep" => Some(Self::Deep),
            "fold" => Some(Self::Fold),
            "toon" => Some(Self::Toon),
            "grep" => Some(Self::Grep),
            "diag" => Some(Self::Diag),
            "tmpl" => Some(Self::Tmpl),
            "squeeze" => Some(Self::Squeeze),
            "mosaic" => Some(Self::Mosaic),
            "identity" => Some(Self::Identity),
            "structural" => Some(Self::Structural),
            "fold-grep-guarded" => Some(Self::FoldGrepGuarded),
            "squeeze-stage1" => Some(Self::SqueezeStage1),
            "squeeze-mine-guarded" => Some(Self::SqueezeMineGuarded),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mine => "mine",
            Self::Deep => "deep",
            Self::Fold => "fold",
            Self::Toon => "toon",
            Self::Grep => "grep",
            Self::Diag => "diag",
            Self::Tmpl => "tmpl",
            Self::Squeeze => "squeeze",
            Self::Mosaic => "mosaic",
            Self::Identity => "identity",
            Self::Structural => "structural",
            Self::FoldGrepGuarded => "fold-grep-guarded",
            Self::SqueezeStage1 => "squeeze-stage1",
            Self::SqueezeMineGuarded => "squeeze-mine-guarded",
        }
    }
}

/// Profile-learned seeds for `encode_seeded` (`qodec learn`): phrases join
/// the miners' probe queue ahead of discovery, templates pre-shape `tmpl`
/// clustering. Both are suggestions — every use is measured, so a stale
/// profile can waste probes, never size or bytes.
#[derive(Debug, Clone, Default)]
pub struct Seeds {
    pub phrases: Vec<String>,
    /// tmpl templates as fixed parts, wildcards between consecutive parts.
    pub templates: Vec<Vec<String>>,
    /// Trained probe ranker (`qodec train`): reorders the miners' probe
    /// queue by predicted gain. Ordering only, acceptance stays measured.
    pub ranker: Option<rank::Ranker>,
    /// Measured probes per mining round (`None` = the default 40). With a
    /// good ranker a small budget keeps the ratio at a fraction of the CPU.
    pub probe_budget: Option<usize>,
}

pub fn encode(text: &str, kind: CodecKind, meter: &dyn TokenMeter, alphabet: Alphabet) -> String {
    encode_seeded(text, kind, meter, alphabet, &Seeds::default())
}

/// `encode` with profile-learned seeds (`qodec learn`). Seeds are tried
/// ahead of same-run discovery; acceptance stays measured, so they can
/// only change what gets tried first, never what survives.
pub fn encode_seeded(
    text: &str,
    kind: CodecKind,
    meter: &dyn TokenMeter,
    alphabet: Alphabet,
    seeds: &Seeds,
) -> String {
    let defaults = MineOptions::default();
    let mine_opts = MineOptions {
        alphabet,
        seeds: seeds.phrases.clone(),
        ranker: seeds.ranker.clone(),
        probe_budget: seeds.probe_budget.unwrap_or(defaults.probe_budget),
        ..MineOptions::default()
    };
    let deep_opts = MineOptions {
        alphabet,
        miner: MinerKind::Deep,
        seeds: seeds.phrases.clone(),
        ranker: seeds.ranker.clone(),
        probe_budget: seeds.probe_budget.unwrap_or(defaults.probe_budget),
        ..MineOptions::default()
    };
    match kind {
        CodecKind::Mine => mine::encode(text, meter, &mine_opts),
        CodecKind::Deep => mine::encode(text, meter, &deep_opts),
        CodecKind::Fold => fold::encode(text, meter),
        CodecKind::Toon => toon::encode(text, meter),
        CodecKind::Grep => grep::encode(text, meter),
        CodecKind::Diag => diag::encode(text, meter),
        CodecKind::Tmpl => tmpl::encode_seeded(text, meter, &seeds.templates),
        CodecKind::Squeeze => squeeze_encode(text, meter, &seeds.templates, &mine_opts, &deep_opts),
        CodecKind::FoldGrepGuarded => {
            // VG: the VERBATIM structural stage (fold/grep only — never the
            // alias-legend codecs tmpl/diag), then a guarded mine so no generic
            // lexical span is ever aliased by either stage. squeeze is untouched.
            let guarded = MineOptions {
                alphabet,
                seeds: seeds.phrases.clone(),
                ranker: seeds.ranker.clone(),
                probe_budget: seeds.probe_budget.unwrap_or(defaults.probe_budget),
                guard_lexical: true,
                ..MineOptions::default()
            };
            let guarded_deep = MineOptions {
                alphabet,
                miner: MinerKind::Deep,
                seeds: seeds.phrases.clone(),
                ranker: seeds.ranker.clone(),
                probe_budget: seeds.probe_budget.unwrap_or(defaults.probe_budget),
                guard_lexical: true,
                ..MineOptions::default()
            };
            let stage1 = structural_stage(text, meter);
            let best = mine_over(&stage1, meter, &guarded, &guarded_deep);
            if meter.count(&best) < meter.count(text) {
                best
            } else {
                container::raw(text)
            }
        }
        CodecKind::SqueezeStage1 => squeeze_stage1(text, meter, &seeds.templates),
        CodecKind::SqueezeMineGuarded => {
            // SG: production stage 1 (shared code) + a GUARDED mine. Same stage-1
            // artifact as squeeze/SM, so SM and SG differ only in the guard.
            let guarded = MineOptions {
                alphabet,
                seeds: seeds.phrases.clone(),
                ranker: seeds.ranker.clone(),
                probe_budget: seeds.probe_budget.unwrap_or(defaults.probe_budget),
                guard_lexical: true,
                ..MineOptions::default()
            };
            let guarded_deep = MineOptions {
                alphabet,
                miner: MinerKind::Deep,
                seeds: seeds.phrases.clone(),
                ranker: seeds.ranker.clone(),
                probe_budget: seeds.probe_budget.unwrap_or(defaults.probe_budget),
                guard_lexical: true,
                ..MineOptions::default()
            };
            let stage1 = squeeze_stage1(text, meter, &seeds.templates);
            let best = mine_over(&stage1, meter, &guarded, &guarded_deep);
            if meter.count(&best) < meter.count(text) {
                best
            } else {
                container::raw(text)
            }
        }
        CodecKind::Identity => container::identity(text),
        CodecKind::Structural => {
            // Structural folding/grouping only — no mine stage, verbatim content.
            let stage = structural_stage(text, meter);
            if meter.count(&stage) < meter.count(text) {
                stage
            } else {
                container::raw(text)
            }
        }
        CodecKind::Mosaic => {
            // Stage 1: route each region to its cheapest structural codec via a
            // measured shortest path. Stage 2 (shared with squeeze): mine the
            // whole assembled mosaic — repeated nested `%q1` headers and legend
            // fragments across siblings are fair game. Final acceptance is the
            // exact meter vs the original, so an approximate DP path can only
            // waste probes, never bytes.
            let stage1 = mosaic::encode_seeded(text, meter, &seeds.templates);
            let best = mine_over(&stage1, meter, &mine_opts, &deep_opts);
            if meter.count(&best) < meter.count(text) {
                best
            } else {
                container::raw(text)
            }
        }
    }
}

/// Production squeeze's EXACT stage-1 selection: `toon` for table-shaped JSON,
/// otherwise the measured best of fold/grep/diag/tmpl. This is the one place the
/// selection lives, so the eval `squeeze-stage1` (S) and `squeeze-mine-guarded`
/// (SG) arms share production's code rather than a copy — an SG built on this and
/// an SM (== squeeze) built on this differ only in the mine guard.
pub fn squeeze_stage1(text: &str, meter: &dyn TokenMeter, templates: &[Vec<String>]) -> String {
    if serde_json::from_str::<serde_json::Value>(text).is_ok() {
        let tooned = toon::encode(text, meter);
        if container::parse(&tooned).ok().is_none_or(|c| c.codec == "raw") {
            best_text_stage(text, meter, templates)
        } else {
            tooned
        }
    } else {
        best_text_stage(text, meter, templates)
    }
}

/// Squeeze's full pipeline, shared by `squeeze` and `fold-grep-guarded` (which
/// pass guarded mine options). Kept byte-for-byte identical to the original
/// inline body so production `squeeze` is unchanged.
fn squeeze_encode(
    text: &str,
    meter: &dyn TokenMeter,
    templates: &[Vec<String>],
    mine_opts: &MineOptions,
    deep_opts: &MineOptions,
) -> String {
    let stage1 = squeeze_stage1(text, meter, templates);
    let best = mine_over(&stage1, meter, mine_opts, deep_opts);
    if meter.count(&best) < meter.count(text) {
        best
    } else {
        container::raw(text)
    }
}

/// Shared stage-2 for the pipeline codecs: mine the assembled container with
/// both miners and keep whichever measures strictly cheaper than the input
/// container (mining a raw container's overhead can otherwise "win" while
/// losing to the plain stage-1).
fn mine_over(
    stage1: &str,
    meter: &dyn TokenMeter,
    mine_opts: &MineOptions,
    deep_opts: &MineOptions,
) -> String {
    let stage2 = [
        mine::encode(stage1, meter, mine_opts),
        mine::encode(stage1, meter, deep_opts),
    ]
    .into_iter()
    .min_by_key(|artifact| meter.count(artifact))
    .unwrap_or_else(|| stage1.to_string());
    if meter.count(&stage2) < meter.count(stage1) {
        stage2
    } else {
        stage1.to_string()
    }
}

/// Verbatim structural stage for the ablation F / GF arms: the measured cheaper
/// of `fold` / `grep` ONLY. Excludes `diag`/`tmpl`, which substitute via an
/// alias legend — factor purity (no aliasing) over compression ratio. Both fold
/// and grep keep paths and identifiers verbatim.
fn structural_stage(text: &str, meter: &dyn TokenMeter) -> String {
    [fold::encode(text, meter), grep::encode(text, meter)]
        .into_iter()
        .min_by_key(|artifact| meter.count(artifact))
        .unwrap_or_else(|| container::raw(text))
}

/// Squeeze's text stage: every structural text codec is one linear pass
/// and refuses honestly, so the measured minimum is always safe to take.
fn best_text_stage(text: &str, meter: &dyn TokenMeter, templates: &[Vec<String>]) -> String {
    [
        fold::encode(text, meter),
        grep::encode(text, meter),
        diag::encode(text, meter),
        tmpl::encode_seeded(text, meter, templates),
    ]
    .into_iter()
    .min_by_key(|artifact| meter.count(artifact))
    .unwrap_or_else(|| container::raw(text))
}

/// The out-of-band keys an artifact may pin: the phrase legend (`ext`
/// wrapper) and the template legend (`ext=` param on a tmpl container).
/// Both live in a cached prompt prefix on the reader's side; decode fails
/// closed on a pinned key that is missing or drifted.
#[derive(Default)]
pub struct Keys<'a> {
    pub phrases: Option<&'a legend::ExternLegend>,
    pub templates: Option<&'a legend::TemplateLegend>,
    /// Verified proposer rules (`qodec rules verify`) — parametric span
    /// rewrites applied as an encode pre-pass, pinned like the legends.
    pub rules: Option<&'a rules::RulesKey>,
}

/// Decode one container layer.
pub fn decode_once(text: &str) -> Result<String> {
    decode_container(&container::parse(text)?, &Keys::default())
}

fn decode_container(c: &container::Container, keys: &Keys<'_>) -> Result<String> {
    match c.codec.as_str() {
        "raw" => Ok(c.body.clone()),
        "identity" => Ok(c.body.clone()),
        "mine" => mine::decode(c),
        "fold" => fold::decode(c),
        "toon" => toon::decode(c),
        "grep" => grep::decode(c),
        "diag" => diag::decode(c),
        "tmpl" => tmpl::decode(c, keys.templates),
        "mosaic" => {
            // Each segment is a single-layer container by construction, so
            // decode exactly one layer per segment — no `decode_all` loop,
            // which keeps mosaic from amplifying the already-container-shaped
            // over-unwrap caveat across many siblings.
            let mut out = String::new();
            for seg in mosaic::split(c)? {
                let inner = container::parse(&seg)?;
                if inner.codec == "mosaic" {
                    // v1 segments are single structural layers by construction;
                    // a hand-built nested mosaic could otherwise recurse a
                    // decoder to stack exhaustion. Refuse it outright.
                    bail!("mosaic: nested mosaic segments are unsupported");
                }
                out.push_str(&decode_container(&inner, keys)?);
            }
            Ok(out)
        }
        "ext" => bail!(
            "artifact was encoded against an extern legend — decode with \
             --extern-legend <file> (sum={})",
            c.param("sum").unwrap_or("?"),
        ),
        "rules" => bail!(
            "artifact was encoded against a rules key — decode with \
             --rules <file> (sum={})",
            c.param("sum").unwrap_or("?"),
        ),
        other => bail!("unknown codec {other:?} in container"),
    }
}

/// `decode` that can open `ext` artifacts: the outer container pins an
/// extern legend by checksum; the inner artifact decodes normally, then
/// the used aliases expand from the supplied legend. Without the exact
/// legend this fails closed instead of reconstructing wrong bytes.
pub fn decode_with_extern(
    text: &str,
    extern_legend: Option<&legend::ExternLegend>,
) -> Result<String> {
    decode_with_keys(
        text,
        &Keys {
            phrases: extern_legend,
            ..Keys::default()
        },
    )
}

/// `decode` with the full key ring: opens phrase-`ext` wrappers and
/// extern-template tmpl artifacts, composed or alone.
pub fn decode_with_keys(text: &str, keys: &Keys<'_>) -> Result<String> {
    match container::parse(text) {
        Ok(c) if c.codec == "ext" => {
            let inner = decode_all(&c.body, keys)?;
            legend::expand(&c, &inner, keys.phrases)
        }
        Ok(c) if c.codec == "rules" => {
            let inner = decode_all(&c.body, keys)?;
            rules::expand(&c, &inner, keys.rules)
        }
        _ => decode_all(text, keys),
    }
}

/// Decode until the text is no longer a container (unwraps pipelines).
/// Note: input that was *already* container-shaped before encoding will also
/// be unwrapped — a lab-grade caveat, documented in the design doc.
pub fn decode(text: &str) -> Result<String> {
    decode_all(text, &Keys::default())
}

fn decode_all(text: &str, keys: &Keys<'_>) -> Result<String> {
    let mut current = text.to_string();
    loop {
        match container::parse(&current) {
            Ok(c) => current = decode_container(&c, keys)?,
            Err(_) => return Ok(current),
        }
    }
}
