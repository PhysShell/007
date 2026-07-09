//! qodec — Q's codec lab: token-aware lossless encode/decode experiments for
//! agent context. See `docs/token-codec.md` in the repo root for the design
//! record and measured results.

pub mod ab;
pub mod alias;
pub mod bench;
pub mod container;
pub mod fold;
pub mod meter;
pub mod mine;
pub mod ppl;
pub mod sam;
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
    /// Pipeline: `toon` (JSON) or `fold` (text), then the better of the two
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
            Self::Squeeze => "squeeze",
        }
    }
}

pub fn encode(text: &str, kind: CodecKind, meter: &dyn TokenMeter, alphabet: Alphabet) -> String {
    let mine_opts = MineOptions {
        alphabet,
        ..MineOptions::default()
    };
    let deep_opts = MineOptions {
        alphabet,
        miner: MinerKind::Deep,
        ..MineOptions::default()
    };
    match kind {
        CodecKind::Mine => mine::encode(text, meter, &mine_opts),
        CodecKind::Deep => mine::encode(text, meter, &deep_opts),
        CodecKind::Fold => fold::encode(text, meter),
        CodecKind::Toon => toon::encode(text, meter),
        CodecKind::Squeeze => {
            let stage1 = if serde_json::from_str::<serde_json::Value>(text).is_ok() {
                let tooned = toon::encode(text, meter);
                // toon may fall back on non-table JSON — pretty-printed JSON
                // with repeated lines can still benefit from RLE.
                if container::parse(&tooned)
                    .ok()
                    .is_none_or(|c| c.codec == "raw")
                {
                    fold::encode(text, meter)
                } else {
                    tooned
                }
            } else {
                fold::encode(text, meter)
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
        other => bail!("unknown codec {other:?} in container"),
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
