//! Alias alphabets for the dictionary miner.
//!
//! An alias is the short stand-in written into the encoded body in place of a
//! mined phrase. Two families:
//!
//! * **glyph** — single CJK ideographs. Many cost exactly 1 token in modern
//!   BPE vocabularies, which makes them the densest possible alias — but only
//!   *measured* ones are trusted (e.g. in o200k `码` is 1 token, `堆` is 2).
//! * **sigil** — one rare marker char + decimal index (`§0`, `§17`). Costs 2
//!   tokens under o200k but is visually tame and effectively unlimited.
//!
//! The pool probes every candidate through the live [`TokenMeter`] and sorts
//! by measured cost. Nothing is assumed about any tokenizer.

use std::collections::HashSet;

use crate::meter::TokenMeter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alphabet {
    /// Glyphs first (cheapest measured), sigil-indexed as overflow.
    Auto,
    /// Single-ideograph aliases only.
    Glyph,
    /// Sigil-indexed aliases only.
    Sigil,
}

impl Alphabet {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "auto" => Some(Self::Auto),
            "glyph" => Some(Self::Glyph),
            "sigil" => Some(Self::Sigil),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Glyph => "glyph",
            Self::Sigil => "sigil",
        }
    }
}

/// Curated single-char glyph candidates. Common-enough CJK to be cheap in BPE
/// vocabularies, distinct enough to never collide with source text or logs.
/// Order is only a seed — the pool re-sorts by measured cost.
const GLYPHS: &[char] = &[
    '码', '引', '路', '类', '函', '值', '错', '试', '帧', '件', '组', '标', '记', '链', '节', '层',
    '块', '表', '行', '列', '键', '名', '数', '串', '图', '树', '点', '边', '库', '包', '版', '构',
    '建', '测', '例', '警', '告', '异', '常', '态', '流', '程', '入', '出', '参', '回', '调', '查',
    '找', '换', '写', '读', '存', '取', '删', '改', '增', '断', '连', '接', '服', '务', '器', '端',
    '口', '域', '址', '密', '钥', '签', '证', '权', '限', '角', '色',
];

/// Rare marker chars for sigil-indexed aliases; the first one absent from the
/// input text is chosen, so aliases can never collide with source content.
///
/// Sigil indices are **fixed-width** (`§00`–`§99`): no alias is a prefix of
/// another, and since `§` never occurs in the original text, every `§` in an
/// encoded body starts exactly one three-char alias — a literal digit that
/// happens to follow an alias belongs to the data, and exact-string
/// replacement can never misread it. (CodeRabbit review on PR #26: variable-
/// width indices made every later sigil alias unusable or ambiguous.)
const SIGILS: &[char] = &['§', '¤', 'µ', '†', '‡', '¬', 'ø', 'þ'];

pub struct AliasPool {
    /// (alias, measured token cost), sorted cheapest-first.
    entries: Vec<(String, usize)>,
    next: usize,
}

impl AliasPool {
    /// Probe candidates against `meter`, dropping any whose chars occur in
    /// `text` (collision safety is decided against the *original* input; the
    /// miner never introduces pool chars except as committed aliases).
    pub fn build(alphabet: Alphabet, meter: &dyn TokenMeter, text: &str) -> Self {
        let present: HashSet<char> = text.chars().collect();
        let mut entries: Vec<(String, usize)> = Vec::new();

        if matches!(alphabet, Alphabet::Auto | Alphabet::Glyph) {
            for &g in GLYPHS {
                if present.contains(&g) {
                    continue;
                }
                let alias = g.to_string();
                let cost = meter.count(&alias);
                entries.push((alias, cost));
            }
        }

        if matches!(alphabet, Alphabet::Auto | Alphabet::Sigil) {
            if let Some(&sigil) = SIGILS.iter().find(|c| !present.contains(c)) {
                for i in 0..100usize {
                    let alias = format!("{sigil}{i:02}");
                    let cost = meter.count(&alias);
                    entries.push((alias, cost));
                }
            }
        }

        // Stable: cheapest first, glyphs before sigils on ties (shorter body).
        entries.sort_by_key(|(alias, cost)| (*cost, alias.len()));
        Self { entries, next: 0 }
    }

    /// Hand out the next cheapest unused alias. Pool entries are unique, so
    /// sequential handout is all the collision avoidance aliases need.
    pub fn take(&mut self) -> Option<(String, usize)> {
        let entry = self.entries.get(self.next).cloned();
        self.next += 1;
        entry
    }
}

/// Probe report row for the `aliases` subcommand — the "play with your
/// tokenizer" surface.
pub struct ProbeRow {
    pub alias: String,
    pub kind: &'static str,
    pub cost: usize,
}

pub fn probe_table(meter: &dyn TokenMeter, top: usize) -> Vec<ProbeRow> {
    let mut rows: Vec<ProbeRow> = GLYPHS
        .iter()
        .map(|&g| ProbeRow {
            alias: g.to_string(),
            kind: "glyph",
            cost: meter.count(&g.to_string()),
        })
        .collect();
    for &s in SIGILS {
        for i in [0usize, 7, 42] {
            let alias = format!("{s}{i:02}");
            rows.push(ProbeRow {
                cost: meter.count(&alias),
                alias,
                kind: "sigil",
            });
        }
    }
    rows.sort_by_key(|r| (r.cost, r.alias.len()));
    rows.truncate(top);
    rows
}
