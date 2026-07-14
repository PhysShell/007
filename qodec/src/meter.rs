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
    fn name(&self) -> &str;
    fn count(&self, text: &str) -> usize;
    /// `true` once a `count` has failed. A fail-closed meter (the HF tokenizer)
    /// never returns a guessed count on error — it marks itself poisoned and the
    /// CLI aborts with no token result, so a run can never silently proceed on
    /// fabricated numbers. The BPE/char meters cannot fail and stay `false`.
    fn poisoned(&self) -> bool {
        false
    }
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
    fn name(&self) -> &str {
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
    fn name(&self) -> &str {
        "approx"
    }

    fn count(&self, text: &str) -> usize {
        // ceil(chars * 2 / 7) == ceil(chars / 3.5)
        let chars = text.chars().count();
        (chars * 2).div_ceil(7)
    }
}

/// A meter backed by a real model's `tokenizer.json` (the Hugging Face
/// `tokenizers` format — GLM, Qwen, Llama, …). This is what makes Level 2
/// honest: aliases and codec acceptance are chosen under the tokenizer the
/// served model actually reads, not an o200k proxy. In-process (the Rust
/// `tokenizers` crate), so a count costs no subprocess.
pub struct HfMeter {
    name: String,
    tokenizer: tokenizers::Tokenizer,
    /// Interior mutability: `count` takes `&self` but must record a failure.
    /// Single-threaded CLI use, so a `Cell` is enough (no `Sync` needed).
    poisoned: std::cell::Cell<bool>,
}

impl HfMeter {
    /// Load from a `tokenizer.json` path. The meter name is `hf:<path>` so
    /// reports and run records identify which tokenizer produced the numbers.
    /// A probe encode runs at load so a structurally-valid-but-unusable
    /// tokenizer fails here rather than mid-run.
    pub fn from_file(path: &str) -> Result<Self> {
        let tokenizer = tokenizers::Tokenizer::from_file(path)
            .map_err(|e| anyhow::anyhow!("loading tokenizer {path}: {e}"))?;
        tokenizer
            .encode("qodec meter probe", false)
            .map_err(|e| anyhow::anyhow!("tokenizer {path} cannot encode a probe string: {e}"))?;
        Ok(Self {
            name: format!("hf:{path}"),
            tokenizer,
            poisoned: std::cell::Cell::new(false),
        })
    }
}

impl TokenMeter for HfMeter {
    fn name(&self) -> &str {
        &self.name
    }

    fn count(&self, text: &str) -> usize {
        // `add_special_tokens = false`: count the content's own tokens, not the
        // chat-template wrapping the server adds — that is what the codec
        // optimizes and what raw-vs-encoded must be compared on.
        //
        // Fail closed: if the tokenizer cannot encode this input, do NOT guess a
        // char count (that would silently corrupt every downstream measurement
        // and the L2 verdict). Poison the meter and return 0; the CLI checks
        // `poisoned()` and aborts with no token result.
        match self.tokenizer.encode(text, false) {
            Ok(enc) => enc.len(),
            Err(_) => {
                self.poisoned.set(true);
                0
            }
        }
    }

    fn poisoned(&self) -> bool {
        self.poisoned.get()
    }
}

pub fn by_name(name: &str) -> Result<Box<dyn TokenMeter>> {
    if let Some(path) = name.strip_prefix("hf:") {
        return Ok(Box::new(HfMeter::from_file(path)?));
    }
    match name {
        "o200k" => Ok(Box::new(Bpe::o200k()?)),
        "cl100k" => Ok(Box::new(Bpe::cl100k()?)),
        "approx" => Ok(Box::new(Approx)),
        other => bail!(
            "unknown meter {other:?} (expected o200k | cl100k | approx | hf:<tokenizer.json>)"
        ),
    }
}
