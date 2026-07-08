//! Perplexity gate — "compression = prediction", inverted.
//!
//! The nncp/LLMZip end of the compression literature uses an LM's next-token
//! predictions to *compress*. Flip it: if a small local LM finds the encoded
//! body barely harder to predict than the raw text, a big model will very
//! likely read it fine; if perplexity explodes, the artifact is probably
//! model-hostile. That makes a cheap pre-gate before spending real judge
//! runs (`o7 judge`) on comprehension A/B — and it is exactly where
//! FastContext (`docs/fastcontext.md`) plugs in: served locally behind an
//! OpenAI-compatible endpoint.
//!
//! Wire contract: legacy `/v1/completions` with `echo=true, max_tokens=0,
//! logprobs=0` returning `choices[0].logprobs.token_logprobs` for the prompt
//! tokens (vLLM implements this; llama-server partially). Perplexity =
//! exp(−mean logprob) over the non-null entries.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

pub struct PplConfig {
    pub url: String,
    pub model: String,
}

pub struct PplScore {
    pub tokens: usize,
    pub perplexity: f64,
}

pub fn score(cfg: &PplConfig, text: &str) -> Result<PplScore> {
    let request = json!({
        "model": cfg.model,
        "prompt": text,
        "max_tokens": 0,
        "echo": true,
        "logprobs": 0,
    });

    let response = ureq::post(&cfg.url)
        .set("content-type", "application/json")
        .send_string(&request.to_string())
        .with_context(|| format!("calling completion endpoint {}", cfg.url))?
        .into_string()
        .context("reading completion response")?;

    let value: Value = serde_json::from_str(&response).context("parsing completion response")?;
    let logprobs = value
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("logprobs"))
        .and_then(|l| l.get("token_logprobs"))
        .and_then(Value::as_array)
        .context("response has no choices[0].logprobs.token_logprobs")?;

    let mut sum = 0.0f64;
    let mut n = 0usize;
    for lp in logprobs {
        if let Some(v) = lp.as_f64() {
            sum += v;
            n += 1;
        }
    }
    if n == 0 {
        bail!("endpoint returned no scored tokens (echo/logprobs unsupported?)");
    }
    Ok(PplScore {
        tokens: n,
        perplexity: (-sum / n as f64).exp(),
    })
}

/// Compare raw text vs its encoded artifact under the same LM.
///
/// The ratio is the gate signal: near 1.0 the notation costs the model
/// little surprise; a blowup says the artifact is likely model-hostile.
/// Thresholds are a heuristic starting point, not a law — calibrate against
/// real judge-run agreement before trusting them.
pub struct PplReport {
    pub raw: PplScore,
    pub encoded: PplScore,
}

impl PplReport {
    pub fn ratio(&self) -> f64 {
        self.encoded.perplexity / self.raw.perplexity
    }

    pub fn verdict(&self) -> &'static str {
        let r = self.ratio();
        if r <= 1.5 {
            "likely-readable"
        } else if r <= 3.0 {
            "borderline — run the judge A/B"
        } else {
            "likely model-hostile"
        }
    }
}

pub fn compare(cfg: &PplConfig, raw: &str, encoded: &str) -> Result<PplReport> {
    Ok(PplReport {
        raw: score(cfg, raw)?,
        encoded: score(cfg, encoded)?,
    })
}
