//! qodec — Q's codec lab: token-aware lossless encode/decode experiments for
//! agent context. See `docs/token-codec.md` in the repo root for the design
//! record and measured results.

pub mod ab;
pub mod alias;
pub mod bench;
pub mod container;
pub mod diag;
pub mod fold;
pub mod grep;
pub mod legend;
pub mod meter;
pub mod mine;
pub mod ppl;
pub mod profile;
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
        }
    }
}

pub fn encode(text: &str, kind: CodecKind, meter: &dyn TokenMeter, alphabet: Alphabet) -> String {
    encode_seeded(text, kind, meter, alphabet, &[])
}

/// `encode` with profile-learned seed phrases for the miners (`qodec learn`).
/// Seeds are probed ahead of discovery; acceptance stays measured, so they
/// can only change what gets tried first, never what survives.
pub fn encode_seeded(
    text: &str,
    kind: CodecKind,
    meter: &dyn TokenMeter,
    alphabet: Alphabet,
    seeds: &[String],
) -> String {
    let mine_opts = MineOptions {
        alphabet,
        seeds: seeds.to_vec(),
        ..MineOptions::default()
    };
    let deep_opts = MineOptions {
        alphabet,
        miner: MinerKind::Deep,
        seeds: seeds.to_vec(),
        ..MineOptions::default()
    };
    match kind {
        CodecKind::Mine => mine::encode(text, meter, &mine_opts),
        CodecKind::Deep => mine::encode(text, meter, &deep_opts),
        CodecKind::Fold => fold::encode(text, meter),
        CodecKind::Toon => toon::encode(text, meter),
        CodecKind::Grep => grep::encode(text, meter),
        CodecKind::Diag => diag::encode(text, meter),
        CodecKind::Tmpl => tmpl::encode(text, meter),
        CodecKind::Squeeze => {
            let stage1 = if serde_json::from_str::<serde_json::Value>(text).is_ok() {
                let tooned = toon::encode(text, meter);
                // toon may fall back on non-table JSON — pretty-printed JSON
                // with repeated lines can still benefit from the text shapes.
                if container::parse(&tooned)
                    .ok()
                    .is_none_or(|c| c.codec == "raw")
                {
                    best_text_stage(text, meter)
                } else {
                    tooned
                }
            } else {
                best_text_stage(text, meter)
            };
            // Mine over the full stage-1 container (headers, legends, rows —
            // repeated paths inside cells are fair game); keep whichever
            // miner measures cheaper.
            let stage2 = [
                mine::encode(&stage1, meter, &mine_opts),
                mine::encode(&stage1, meter, &deep_opts),
            ]
            .into_iter()
            .min_by_key(|artifact| meter.count(artifact))
            .unwrap_or_else(|| stage1.clone());
            let best = if meter.count(&stage2) < meter.count(&stage1) {
                stage2
            } else {
                stage1
            };
            // Final acceptance vs the *original* — mining a raw container's
            // overhead can beat stage1 while still losing to the input.
            if meter.count(&best) < meter.count(text) {
                best
            } else {
                container::raw(text)
            }
        }
    }
}

/// Squeeze's text stage: every structural text codec is one linear pass
/// and refuses honestly, so the measured minimum is always safe to take.
fn best_text_stage(text: &str, meter: &dyn TokenMeter) -> String {
    [
        fold::encode(text, meter),
        grep::encode(text, meter),
        diag::encode(text, meter),
        tmpl::encode(text, meter),
    ]
    .into_iter()
    .min_by_key(|artifact| meter.count(artifact))
    .unwrap_or_else(|| container::raw(text))
}

/// Decode one container layer.
pub fn decode_once(text: &str) -> Result<String> {
    decode_container(&container::parse(text)?)
}

fn decode_container(c: &container::Container) -> Result<String> {
    match c.codec.as_str() {
        "raw" => Ok(c.body.clone()),
        "mine" => mine::decode(c),
        "fold" => fold::decode(c),
        "toon" => toon::decode(c),
        "grep" => grep::decode(c),
        "diag" => diag::decode(c),
        "tmpl" => tmpl::decode(c),
        "ext" => bail!(
            "artifact was encoded against an extern legend — decode with \
             --extern-legend <file> (sum={})",
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
    match container::parse(text) {
        Ok(c) if c.codec == "ext" => {
            let inner = decode(&c.body)?;
            legend::expand(&c, &inner, extern_legend)
        }
        _ => decode(text),
    }
}

/// Decode until the text is no longer a container (unwraps pipelines).
/// Note: input that was *already* container-shaped before encoding will also
/// be unwrapped — a lab-grade caveat, documented in the design doc.
pub fn decode(text: &str) -> Result<String> {
    let mut current = text.to_string();
    loop {
        match container::parse(&current) {
            Ok(c) => current = decode_container(&c)?,
            Err(_) => return Ok(current),
        }
    }
}
