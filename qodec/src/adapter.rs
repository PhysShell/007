//! Adapter envelope — the contract the interop bench (`qodec/evals/interop/`)
//! calls qodec through.
//!
//! `encode` always wraps: even when nothing mines, it returns a `raw`
//! container that costs ~13 tokens of header. That is the right default for a
//! codec you invoke *because* you expect a win. It is the wrong default for a
//! layered pipeline that applies qodec *blindly* over output another optimizer
//! already compressed (RTK's filtered stdout, a Headroom-shaped prompt, a
//! dense FastContext brief): if the residue holds no repetition, the container
//! tax is a pure loss — the very −4.2% the design doc records on unique prose.
//!
//! `adapt` closes that hole. It compares the artifact against the input under
//! the live meter and, when asked, passes the original through untouched, so
//! qodec can sit at the end of any lane and never worsen what reached it.

use crate::container;
use crate::meter::TokenMeter;

/// The decision an adapter reports for one payload.
#[derive(Debug, Clone)]
pub struct Adapted {
    /// `true` when `content` is a `%q1` artifact the reader must `decode`;
    /// `false` when `content` is the original text, passed through verbatim.
    pub encoded: bool,
    /// The container codec that won (`mine`, `deep`, `toon`, … or `raw`), or
    /// `passthrough` when the original was returned untouched.
    pub codec: String,
    /// The artifact (when `encoded`) or the original text (passthrough).
    pub content: String,
    /// Tokens of the original input under the meter.
    pub tokens_in: usize,
    /// Tokens of `content` under the meter — what the reader actually pays.
    pub tokens_out: usize,
}

impl Adapted {
    /// A win means the reader pays strictly fewer tokens than the raw input.
    /// `passthrough` and a `raw` fallback both report `false` here.
    pub fn is_win(&self) -> bool {
        self.tokens_out < self.tokens_in
    }
}

/// Decide whether the encoded `artifact` is worth sending.
///
/// When the artifact does not strictly beat `input` under `meter` and
/// `passthrough_on_no_gain` is set, return the original untouched
/// (`encoded = false`, `codec = "passthrough"`) so blind application in a
/// layered pipeline can never inflate an already-compressed payload. Without
/// the flag the artifact is kept as produced (the historical always-wrap
/// behavior), so `decode` still recovers the input from its `raw` container.
///
/// `artifact` is taken already-encoded rather than re-encoded here: the caller
/// has built it through whatever key/profile path it chose, and the adapter's
/// only job is the send/passthrough decision on top of that.
pub fn adapt(
    input: &str,
    artifact: &str,
    meter: &dyn TokenMeter,
    passthrough_on_no_gain: bool,
) -> Adapted {
    let tokens_in = meter.count(input);
    let tokens_out = meter.count(artifact);
    if tokens_out >= tokens_in && passthrough_on_no_gain {
        return Adapted {
            encoded: false,
            codec: "passthrough".to_string(),
            content: input.to_string(),
            tokens_in,
            tokens_out: tokens_in,
        };
    }
    let codec = container::parse(artifact)
        .map(|c| c.codec)
        .unwrap_or_else(|_| "raw".to_string());
    Adapted {
        encoded: true,
        codec,
        content: artifact.to_string(),
        tokens_in,
        tokens_out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::Alphabet;
    use crate::meter::Bpe;
    use crate::{encode, CodecKind};

    fn repetitive() -> String {
        let mut text = String::new();
        for name in [
            "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta",
        ] {
            text.push_str(&format!(
                "src/Legacy.UI/ViewModels/{name}ViewModel.cs uses ConfigureAwait(false) and CancellationToken cancellationToken\n"
            ));
        }
        text
    }

    #[test]
    fn win_keeps_the_artifact_and_reports_the_codec() -> anyhow::Result<()> {
        let meter = Bpe::o200k()?;
        let text = repetitive();
        let artifact = encode(&text, CodecKind::Mine, &meter, Alphabet::Auto);
        let out = adapt(&text, &artifact, &meter, true);
        anyhow::ensure!(out.encoded, "a mined win must stay encoded");
        anyhow::ensure!(
            out.codec == "mine",
            "codec should be mine, got {}",
            out.codec
        );
        anyhow::ensure!(out.content == artifact, "content must be the artifact");
        anyhow::ensure!(out.is_win() && out.tokens_out < out.tokens_in);
        Ok(())
    }

    #[test]
    fn passthrough_returns_the_original_untouched_on_no_gain() -> anyhow::Result<()> {
        let meter = Bpe::o200k()?;
        let text = "one two three four five six seven eight nine ten.\n";
        let artifact = encode(text, CodecKind::Squeeze, &meter, Alphabet::Auto);
        // Sanity: unique prose does not mine, so the container is a pure tax.
        anyhow::ensure!(meter.count(&artifact) >= meter.count(text));
        let out = adapt(text, &artifact, &meter, true);
        anyhow::ensure!(!out.encoded, "no-gain passthrough must not claim encoded");
        anyhow::ensure!(out.codec == "passthrough");
        anyhow::ensure!(
            out.content == text,
            "content must be byte-identical to input"
        );
        anyhow::ensure!(
            out.tokens_out == out.tokens_in,
            "passthrough pays exactly the input"
        );
        anyhow::ensure!(!out.is_win());
        Ok(())
    }

    #[test]
    fn without_the_flag_no_gain_keeps_the_raw_container() -> anyhow::Result<()> {
        let meter = Bpe::o200k()?;
        let text = "one two three four five six seven eight nine ten.\n";
        let artifact = encode(text, CodecKind::Squeeze, &meter, Alphabet::Auto);
        let out = adapt(text, &artifact, &meter, false);
        anyhow::ensure!(out.encoded, "always-wrap: content is still a container");
        anyhow::ensure!(
            out.codec == "raw",
            "no-gain container is raw, got {}",
            out.codec
        );
        anyhow::ensure!(out.content == artifact);
        anyhow::ensure!(!out.is_win(), "a raw container never wins on tokens");
        Ok(())
    }
}
