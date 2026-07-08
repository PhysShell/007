//! Suffix automaton — the full-strength repeat miner.
//!
//! The BWT-family lesson (see `docs/token-codec.md`, "BWT lineage"): real
//! repetition ignores human-visible boundaries — syllables, not words. The
//! word-boundary miner and its separator-prefix extension are cheap
//! approximations; the suffix automaton sees *every* repeated substring in
//! O(n) states, at the classic ratio-vs-CPU trade the bzip literature keeps
//! rediscovering.
//!
//! Built over bytes; extraction re-checks UTF-8 char boundaries before a
//! candidate leaves this module. Occurrence counts are endpos-set sizes
//! (overlap-blind) — good enough for ranking, since the miner's commit
//! decision re-measures the actual replacement against the live tokenizer.

use std::collections::HashMap;

struct State {
    /// Length of the longest substring in this endpos class.
    len: usize,
    /// Suffix link (`usize::MAX` = none, root only).
    link: usize,
    /// Transitions.
    next: HashMap<u8, usize>,
    /// Occurrence count (endpos-set size after propagation).
    count: usize,
    /// One end offset (exclusive) of the longest substring, for extraction.
    end: usize,
}

const NONE: usize = usize::MAX;

pub struct Candidate {
    pub text: String,
    /// Approximate (overlap-blind) occurrence count.
    pub count: usize,
}

/// All repeated substrings of `text` within the length window, ranked by the
/// (count × len) proxy, deduplicated, best `top` returned.
///
/// Indexing here is proven safe by construction — every index stored in
/// `link`/`next`/`last` was produced by a `states.push` earlier in the same
/// build — so the file-wide strict lint gets one justified exception.
#[allow(clippy::indexing_slicing)]
pub fn repeated_substrings(
    text: &str,
    min_len: usize,
    max_len: usize,
    top: usize,
) -> Vec<Candidate> {
    let bytes = text.as_bytes();
    if bytes.len() < min_len * 2 {
        return Vec::new();
    }

    // --- build (classic online SAM) ---
    let mut states: Vec<State> = vec![State {
        len: 0,
        link: NONE,
        next: HashMap::new(),
        count: 0,
        end: 0,
    }];
    let mut last = 0usize;

    for (i, &ch) in bytes.iter().enumerate() {
        let cur = states.len();
        states.push(State {
            len: i + 1,
            link: NONE,
            next: HashMap::new(),
            count: 1, // a real position; clones start at 0
            end: i + 1,
        });

        let mut p = last;
        while p != NONE && !states[p].next.contains_key(&ch) {
            states[p].next.insert(ch, cur);
            p = states[p].link;
        }

        if p == NONE {
            states[cur].link = 0;
        } else {
            let q = states[p].next[&ch];
            if states[p].len + 1 == states[q].len {
                states[cur].link = q;
            } else {
                let clone = states.len();
                states.push(State {
                    len: states[p].len + 1,
                    link: states[q].link,
                    next: states[q].next.clone(),
                    count: 0,
                    end: states[q].end,
                });
                while p != NONE && states[p].next.get(&ch) == Some(&q) {
                    states[p].next.insert(ch, clone);
                    p = states[p].link;
                }
                states[q].link = clone;
                states[cur].link = clone;
            }
        }
        last = cur;
    }

    // --- propagate counts in len-descending (topological) order ---
    let mut order: Vec<usize> = (1..states.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(states[i].len));
    for &i in &order {
        let link = states[i].link;
        if link != NONE && link != 0 {
            let count = states[i].count;
            states[link].count += count;
        }
    }

    // --- extract candidates ---
    let mut best: HashMap<&str, usize> = HashMap::new();
    for s in states.iter().skip(1) {
        if s.count < 2 || s.len < min_len {
            continue;
        }
        // The state's longest substring; clip to the window.
        let len = s.len.min(max_len);
        let Some(start) = s.end.checked_sub(len) else {
            continue;
        };
        if !text.is_char_boundary(start) || !text.is_char_boundary(s.end) {
            continue;
        }
        let Some(sub) = text.get(start..s.end) else {
            continue;
        };
        // `\r` is banned as well: container parse trims a trailing CR from
        // legend lines (CRLF tolerance), which would corrupt such a phrase.
        if sub.contains('\n') || sub.contains('\r') || sub.trim().is_empty() {
            continue;
        }
        let entry = best.entry(sub).or_insert(0);
        *entry = (*entry).max(s.count);
    }

    let mut ranked: Vec<(&str, usize)> = best.into_iter().collect();
    ranked.sort_by(|a, b| (b.1 * b.0.len(), b.0).cmp(&(a.1 * a.0.len(), a.0)));
    ranked
        .into_iter()
        .take(top)
        .map(|(sub, count)| Candidate {
            text: sub.to_string(),
            count,
        })
        .collect()
}
