//! `profile` — a per-repo redundancy memory, accumulated across runs.
//!
//! The miners re-discover the same paths, type names and message templates
//! on every invocation; a profile remembers them (`qodec learn`), so later
//! encodes probe the known repeats *first* — cross-file knowledge the
//! single-payload miner cannot have. Consumption is seed-only by design:
//! seeds join the candidate queue ahead of discovery, but every commit
//! still passes the same measured acceptance, so a stale profile can waste
//! probes, never correctness. This is the session-dictionary rung of the
//! design record's roadmap (docs/token-codec.md), and the substrate the
//! propose/verify loop stores its survivors in.
//!
//! On-disk format is plain JSON, versioned, deterministically capped and
//! ordered — diffs of a committed profile stay reviewable.

use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::mine;
use crate::tmpl;

const VERSION: u64 = 1;
const MAX_PHRASES: usize = 512;
const MAX_TEMPLATES: usize = 256;
/// Phrases shorter than this never pay for a legend line.
const MIN_PHRASE_CHARS: usize = 6;
/// How many candidates one `learn` pass may harvest from a single text.
const HARVEST_BUDGET: usize = 128;

#[derive(Debug, Default)]
pub struct Profile {
    /// How many texts have ever been learned into this profile.
    pub runs: u64,
    /// Repeated phrases with accumulated occurrence counts.
    phrases: Vec<(String, u64)>,
    /// tmpl templates as their fixed parts (wildcards between parts).
    templates: Vec<(Vec<String>, u64)>,
}

impl Profile {
    /// Load a profile, or start fresh when the file does not exist yet.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let root: Value = serde_json::from_str(&text)
            .with_context(|| format!("parsing profile {}", path.display()))?;
        let version = root.get("v").and_then(Value::as_u64).unwrap_or(0);
        if version != VERSION {
            bail!(
                "profile {} has version {version}, this build reads {VERSION}",
                path.display()
            );
        }
        let mut profile = Self {
            runs: root.get("runs").and_then(Value::as_u64).unwrap_or(0),
            ..Self::default()
        };
        for entry in root
            .get("phrases")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(text), Some(count)) = (
                entry.get(0).and_then(Value::as_str),
                entry.get(1).and_then(Value::as_u64),
            ) else {
                bail!("malformed phrase entry in {}", path.display());
            };
            profile.phrases.push((text.to_string(), count));
        }
        for entry in root
            .get("templates")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(parts), Some(count)) = (
                entry.get(0).and_then(Value::as_array),
                entry.get(1).and_then(Value::as_u64),
            ) else {
                bail!("malformed template entry in {}", path.display());
            };
            let parts: Option<Vec<String>> = parts
                .iter()
                .map(|p| p.as_str().map(str::to_string))
                .collect();
            let parts = parts.with_context(|| format!("malformed template part in {}", path.display()))?;
            profile.templates.push((parts, count));
        }
        Ok(profile)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let phrases: Vec<Value> = self
            .phrases
            .iter()
            .map(|(text, count)| json!([text, count]))
            .collect();
        let templates: Vec<Value> = self
            .templates
            .iter()
            .map(|(parts, count)| json!([parts, count]))
            .collect();
        let root = json!({
            "v": VERSION,
            "runs": self.runs,
            "phrases": phrases,
            "templates": templates,
        });
        let text = serde_json::to_string_pretty(&root).context("serializing profile")?;
        std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
    }

    /// Harvest one text into the profile: repeated phrases (the miner's
    /// word/prefix candidates) and learned tmpl templates, counts merged
    /// with what previous runs saw.
    pub fn learn_from(&mut self, text: &str) {
        self.runs += 1;
        for phrase in mine::learn_phrases(text, HARVEST_BUDGET) {
            if phrase.len() < MIN_PHRASE_CHARS {
                continue;
            }
            let count = text.matches(&phrase).count() as u64;
            merge(&mut self.phrases, phrase, count.max(1));
        }
        for (parts, count) in tmpl::learn_templates(text) {
            merge(&mut self.templates, parts, count as u64);
        }
        self.compact();
    }

    /// Deterministic order + caps: heaviest (count × length) first, ties by
    /// content, so a committed profile diffs cleanly between runs.
    fn compact(&mut self) {
        self.phrases
            .sort_by(|a, b| weight_phrase(b).cmp(&weight_phrase(a)).then(a.0.cmp(&b.0)));
        self.phrases.truncate(MAX_PHRASES);
        self.templates
            .sort_by(|a, b| weight_template(b).cmp(&weight_template(a)).then(a.0.cmp(&b.0)));
        self.templates.truncate(MAX_TEMPLATES);
    }

    /// Seed candidates for the miner: learned phrases and the long fixed
    /// parts of learned templates (paths and sentence stems live there),
    /// merged into one weight order — a heavy template stem must outrank
    /// the cloud of its own sub-span n-grams. Every seed still faces the
    /// measured probe.
    pub fn seed_phrases(&self, top: usize) -> Vec<String> {
        let mut weighted: Vec<(String, u64)> = self
            .phrases
            .iter()
            .map(|(text, count)| (text.clone(), count.saturating_mul(text.len() as u64)))
            .collect();
        for (parts, count) in &self.templates {
            for part in parts {
                let trimmed = part.trim();
                if trimmed.len() >= MIN_PHRASE_CHARS {
                    weighted.push((
                        trimmed.to_string(),
                        count.saturating_mul(trimmed.len() as u64),
                    ));
                }
            }
        }
        weighted.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut seeds: Vec<String> = Vec::new();
        for (text, _) in weighted {
            if seeds.len() >= top {
                break;
            }
            if !seeds.contains(&text) {
                seeds.push(text);
            }
        }
        seeds
    }

    pub fn phrase_count(&self) -> usize {
        self.phrases.len()
    }

    pub fn template_count(&self) -> usize {
        self.templates.len()
    }
}

/// Read at most `cap` bytes of UTF-8 from `path`, returning the text and
/// whether it was capped. The whole point of the cap is to never hold a
/// multi-GB blob in memory, so the bound is applied at the *read*, not
/// after a full `read_to_string` (Codex review on PR #34). A char split
/// by the cap is dropped; invalid UTF-8 anywhere else is an error — the
/// same skip semantics the uncapped read had.
pub fn read_capped(path: &Path, cap: usize) -> Result<(String, bool)> {
    let file =
        std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut buf = Vec::new();
    // One extra byte so "exactly cap bytes" and "capped" are distinguishable.
    file.take(cap as u64 + 1)
        .read_to_end(&mut buf)
        .with_context(|| format!("reading {}", path.display()))?;
    let capped = buf.len() > cap;
    buf.truncate(cap);
    match String::from_utf8(buf) {
        Ok(text) => Ok((text, capped)),
        // Only a char sliced by the cap itself may be dropped (a UTF-8
        // char is at most 4 bytes); earlier invalid bytes mean the file
        // is not text and must be skipped, not silently mangled.
        Err(err) if capped && err.utf8_error().valid_up_to() + 4 > cap => {
            let valid = err.utf8_error().valid_up_to();
            let mut bytes = err.into_bytes();
            bytes.truncate(valid);
            Ok((String::from_utf8(bytes).unwrap_or_default(), true))
        }
        Err(err) => bail!("{} is not UTF-8: {err}", path.display()),
    }
}

fn merge<T: PartialEq>(entries: &mut Vec<(T, u64)>, key: T, add: u64) {
    match entries.iter_mut().find(|(k, _)| *k == key) {
        Some((_, count)) => *count += add,
        None => entries.push((key, add)),
    }
}

fn weight_phrase(entry: &(String, u64)) -> u64 {
    entry.1.saturating_mul(entry.0.len() as u64)
}

fn weight_template(entry: &(Vec<String>, u64)) -> u64 {
    let len: usize = entry.0.iter().map(String::len).sum();
    entry.1.saturating_mul(len as u64)
}
