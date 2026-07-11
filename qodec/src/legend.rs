//! `legend` — the extern dictionary: a stable `alias=phrase` file that
//! lives in a *cached prompt prefix* (CLAUDE.md, system prompt, subagent
//! preamble) instead of traveling inside every artifact.
//!
//! This is the second half of the design record's session-dictionary rung
//! (`docs/token-codec.md` #3; `qodec learn` is the first half): the profile
//! knows a repo's heavy phrases, `qodec legend` freezes the heaviest into a
//! reviewable file with probed one-token aliases, and `encode
//! --extern-legend` substitutes them *without paying for legend lines* —
//! the reader already holds the key. `decode` fails closed: an `ext`
//! artifact without the exact legend file (FNV-checksummed) refuses rather
//! than reconstructing wrong bytes.
//!
//! Safety against collisions is positional, like everywhere in this lab:
//! an entry whose alias glyph already occurs in the input is *skipped* and
//! excluded from the artifact's `used` list, so expansion can never touch
//! bytes the encoder did not substitute.

use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::container::{self, Container};
use crate::meter::TokenMeter;
use crate::profile::Profile;

const HEADER: &str = "# qodec extern legend v1";

/// A parsed extern legend: ordered entries plus the checksum of the exact
/// file bytes (the artifact pins it, decode verifies it).
#[derive(Debug)]
pub struct ExternLegend {
    pub entries: Vec<(String, String)>,
    pub sum: String,
}

impl ExternLegend {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some(HEADER) {
            bail!("not an extern legend (first line must be {HEADER:?})");
        }
        let mut entries: Vec<(String, String)> = Vec::new();
        for line in lines {
            let line = line.trim_end_matches('\r');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((alias, phrase)) = line.split_once('=') else {
                bail!("malformed legend line {line:?}");
            };
            if alias.is_empty() || phrase.is_empty() {
                bail!("empty alias or phrase in legend line {line:?}");
            }
            // A hand-edited or merged legend with two entries per alias
            // would silently reconstruct the wrong phrase on expand — the
            // artifact's `used` list is flat and cannot tell them apart
            // (Codex review on PR #35). Refuse up front.
            if entries.iter().any(|(a, _)| a == alias) {
                bail!("duplicate alias {alias:?} in extern legend");
            }
            entries.push((alias.to_string(), phrase.to_string()));
        }
        if entries.is_empty() {
            bail!("extern legend has no entries");
        }
        Ok(Self {
            sum: fnv1a(text.as_bytes()),
            entries,
        })
    }
}

/// Freeze the heaviest profile phrases into legend text: probed one-token
/// aliases, longest-weight phrases first (the application order — an early
/// long phrase must not be shadowed by a later substring).
pub fn generate(profile: &Profile, meter: &dyn TokenMeter, top: usize) -> Result<String> {
    let phrases = profile.seed_phrases(top);
    if phrases.is_empty() {
        bail!("profile has nothing to freeze — run `qodec learn` first");
    }
    // The pool must avoid every character any phrase contains.
    let exclusion = phrases.join("");
    let mut pool = crate::alias::AliasPool::build(crate::alias::Alphabet::Auto, meter, &exclusion);
    let mut out = String::new();
    out.push_str(HEADER);
    out.push('\n');
    out.push_str("# generated from a qodec profile; keep this file stable —\n");
    out.push_str("# artifacts pin its checksum and fail closed on drift.\n");
    let mut emitted = 0usize;
    for phrase in phrases {
        // Entries are one line each; the parser splits on the FIRST `=`,
        // so `=` inside a phrase is fine — only a newline can break the
        // line format (defensive: profile phrases are single-line).
        if phrase.contains('\n') {
            continue;
        }
        let Some((alias, alias_cost)) = pool.take() else { break };
        // Only freeze entries that actually pay standalone; the encode-time
        // substitution re-measures on the real payload anyway.
        if alias_cost >= meter.count(&phrase) {
            continue;
        }
        out.push_str(&alias);
        out.push('=');
        out.push_str(&phrase);
        out.push('\n');
        emitted += 1;
    }
    if emitted == 0 {
        bail!("no profile phrase beat its alias under this meter");
    }
    Ok(out)
}

/// Substitute legend phrases into `text` ahead of any codec. Each applied
/// entry must measure a strict token win on the real payload; the artifact
/// later records exactly which aliases were used.
pub struct Substituted {
    pub text: String,
    /// Aliases actually applied, in application (file) order.
    pub used: Vec<String>,
}

pub fn substitute(text: &str, legend: &ExternLegend, meter: &dyn TokenMeter) -> Substituted {
    let mut current = text.to_string();
    let mut current_tokens = meter.count(&current);
    let mut used = Vec::new();
    for (alias, phrase) in &legend.entries {
        // A glyph already present in the input (or introduced by an earlier
        // phrase) cannot be told apart from a substitution — skip, and the
        // `used` list keeps decode away from it.
        if current.contains(alias.as_str()) || !current.contains(phrase.as_str()) {
            continue;
        }
        let replaced = current.replace(phrase.as_str(), alias);
        let replaced_tokens = meter.count(&replaced);
        if replaced_tokens < current_tokens {
            current = replaced;
            current_tokens = replaced_tokens;
            used.push(alias.clone());
        }
    }
    Substituted {
        text: current,
        used,
    }
}

/// Wrap an inner artifact in the `ext` container that pins the legend.
pub fn emit(inner: &str, legend: &ExternLegend, used: &[String]) -> String {
    container::emit(&Container {
        codec: "ext".to_string(),
        params: vec![
            ("sum".to_string(), legend.sum.clone()),
            ("used".to_string(), used.concat()),
        ],
        legend: Vec::new(),
        body: inner.to_string(),
    })
}

/// The encode-side gate for the ext wrapper. No substitutions → no
/// wrapper: a stale legend must not turn a normal artifact into one that
/// demands a key it never used (Codex review on PR #35) — with `used`
/// empty, `inner` encoded the untouched original and stands on its own.
/// With substitutions applied, the wrapped artifact must still clear the
/// original's raw floor; when it can't, the only byte-safe fallback is
/// `raw(original)` — `inner` alone would decode to the *substituted* text.
pub fn wrap_if_used(
    inner: String,
    legend: &ExternLegend,
    used: &[String],
    meter: &dyn TokenMeter,
    original: &str,
) -> String {
    if used.is_empty() {
        return inner;
    }
    let wrapped = emit(&inner, legend, used);
    if meter.count(&wrapped) < meter.count(&container::raw(original)) {
        wrapped
    } else {
        container::raw(original)
    }
}

/// Expand an `ext` container body (already inner-decoded) back to original
/// bytes. `legend` must be the exact file the encoder used — fail closed.
pub fn expand(c: &Container, decoded_inner: &str, legend: Option<&ExternLegend>) -> Result<String> {
    let sum = c.param("sum").context("ext container missing sum")?;
    let Some(legend) = legend else {
        bail!(
            "artifact was encoded against an extern legend (sum={sum}); \
             pass --extern-legend with that exact file"
        );
    };
    if legend.sum != sum {
        bail!(
            "extern legend mismatch: artifact pins sum={sum}, file has {} — \
             refusing to reconstruct wrong bytes",
            legend.sum
        );
    }
    let used = c.param("used").unwrap_or_default();
    let mut out = decoded_inner.to_string();
    for (alias, phrase) in legend.entries.iter().rev() {
        if used.contains(alias.as_str()) {
            out = out.replace(alias.as_str(), phrase.as_str());
        }
    }
    Ok(out)
}

/// FNV-1a 64 over raw bytes — a stable, dependency-free fingerprint. Not
/// cryptographic; it guards against version drift, not adversaries.
fn fnv1a(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
