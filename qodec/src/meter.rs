//! Token meters — the only ground truth in this lab.
//!
//! Every codec decision is *measured* against a real tokenizer, never
//! estimated from bytes or characters. Claude's tokenizer is not public, so
//! `o200k` (GPT-4o/o1 family BPE, bundled offline by tiktoken-rs) serves as
//! the default proxy; the relative ordering of codec outcomes is what
//! transfers across BPE tokenizers, and the trait keeps the door open for an
//! API-backed Anthropic meter later.

use anyhow::{bail, Result};

pub trait TokenMeter {
    fn name(&self) -> &'static str;
    fn count(&self, text: &str) -> usize;
}

pub struct Bpe {
    name: &'static str,
    bpe: tiktoken_rs::CoreBPE,
}

impl Bpe {
    pub fn o200k() -> Result<Self> {
        Ok(Self {
            name: "o200k",
            bpe: tiktoken_rs::o200k_base()?,
        })
    }

    pub fn cl100k() -> Result<Self> {
        Ok(Self {
            name: "cl100k",
            bpe: tiktoken_rs::cl100k_base()?,
        })
    }
}

impl TokenMeter for Bpe {
    fn name(&self) -> &'static str {
        self.name
    }

    fn count(&self, text: &str) -> usize {
        self.bpe.encode_ordinary(text).len()
    }
}

/// Char-count heuristic (~3.5 chars/token). Only for fast property tests;
/// bench and encode default to a real BPE.
pub struct Approx;

impl TokenMeter for Approx {
    fn name(&self) -> &'static str {
        "approx"
    }

    fn count(&self, text: &str) -> usize {
        // ceil(chars * 2 / 7) == ceil(chars / 3.5)
        let chars = text.chars().count();
        (chars * 2).div_ceil(7)
    }
}

pub fn by_name(name: &str) -> Result<Box<dyn TokenMeter>> {
    match name {
        "o200k" => Ok(Box::new(Bpe::o200k()?)),
        "cl100k" => Ok(Box::new(Bpe::cl100k()?)),
        "approx" => Ok(Box::new(Approx)),
        other => bail!("unknown meter {other:?} (expected o200k | cl100k | approx)"),
    }
}
