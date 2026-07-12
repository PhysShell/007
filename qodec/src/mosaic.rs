//! `mosaic` — measured optimal segmentation across codecs.
//!
//! Where `squeeze` picks *one* structural codec for the whole payload,
//! `mosaic` cuts the payload at line boundaries and routes each region to the
//! codec that measures cheapest *there* — the diagnostic block to `diag`, a
//! repetitive run to `fold`, unique prose to `raw` — then (in `lib`) optionally
//! mines the whole assembled artifact. The split is a shortest path over a DAG
//! of span candidates: a node is a boundary between lines, an edge `i -> j` is
//! the region `[i, j)` encoded by one codec, weighted by the *measured* full
//! token cost of that nested artifact (header, legend and all). The cheapest
//! `0..N` path is the segmentation.
//!
//! This is the disciplined transplant of the DP-over-formats idea (see
//! `docs/token-codec.md`, "mosaic" section): variable span lengths, legend
//! cost already inside each edge weight, no switch-cost constant to
//! approximate, and a final whole-artifact meter that overrules the DP's
//! BPE-additivity approximation — `tok(A+B) != tok(A) + tok(B)`, so the DP only
//! *proposes*, the exact meter *decides*.
//!
//! ## Container — a length-prefixed envelope of sibling `%q1` artifacts
//!
//! ```text
//! %q1 mosaic n=3
//! %q1 body
//! 154
//! <154 bytes of nested q1 artifact>827
//! <827 bytes>93
//! <93 bytes>
//! ```
//!
//! Each segment is a self-contained container. Decode reads the decimal byte
//! length, takes exactly that many bytes, decodes them (one container layer —
//! v1 segments are never pipelines) and concatenates. Byte-exact: the segments
//! partition the input, and every candidate codec here is itself byte-exact.
//!
//! Deliberately deferred (prove the win first, then earn the complexity):
//! `toon` segments (semantic, not byte-exact), per-span `mine`/`deep`, and a
//! shared opcode table that would strip the repeated nested `%q1` headers.

use anyhow::{bail, Context, Result};

use crate::container::{self, Container};
use crate::meter::TokenMeter;
use crate::{diag, fold, grep, tmpl};

/// Above this many lines the `O(N·W)` candidate sweep is not worth it for a lab
/// codec; fall back to a single raw container (the caller still measures it).
const MAX_LINES: usize = 4000;

/// Geometric window sizes (in lines) tried from every start. `1` guarantees
/// the DAG is always connected; the larger sizes let a region grow until a
/// codec has enough lines to learn from and pay for its own header.
const WINDOWS: [usize; 8] = [1, 2, 4, 8, 16, 32, 64, 128];

/// A small nudge per extra segment: the length-prefix line, plus a tie-breaker
/// toward fewer, larger spans. The real per-segment cost — the nested `%q1`
/// header — already rides inside each edge weight; this only breaks ties, and
/// the final whole-artifact meter is the actual judge.
const FRAME_COST: usize = 1;

/// Encode `text` as a `%q1 mosaic` container of per-region artifacts, chosen by
/// a measured shortest path. Falls back to a single raw container when the
/// input is empty or larger than [`MAX_LINES`] (the caller's final acceptance
/// still measures the result against the original either way).
pub fn encode(text: &str, meter: &dyn TokenMeter) -> String {
    match segment(text, meter) {
        Some(segs) => emit(&segs),
        None => container::raw(text),
    }
}

/// The shortest-path segmentation: the ordered per-region artifacts of the
/// cheapest `0..N` path, or `None` when the input is unsegmentable here.
fn segment(text: &str, meter: &dyn TokenMeter) -> Option<Vec<String>> {
    if text.is_empty() {
        return None;
    }
    let units: Vec<&str> = text.split_inclusive('\n').collect();
    let n = units.len();
    if n == 0 || n > MAX_LINES {
        return None;
    }

    // Byte offset where each unit starts; `offsets[n]` is `text.len()`. A span
    // `[i, j)` is the byte range `offsets[i]..offsets[j]` — always on char
    // boundaries because units come from `split_inclusive`.
    let mut offsets = Vec::with_capacity(n + 1);
    let mut acc = 0usize;
    offsets.push(0usize);
    for u in &units {
        acc += u.len();
        offsets.push(acc);
    }

    // Shortest path: `dp[j]` = cheapest measured cost to encode `units[0..j]`;
    // `best[j]` = (span start `i`, chosen artifact) that achieves it.
    let inf = usize::MAX / 4;
    let mut dp = vec![inf; n + 1];
    let mut best: Vec<Option<(usize, String)>> = vec![None; n + 1];
    if let Some(slot) = dp.first_mut() {
        *slot = 0;
    }

    for i in 0..n {
        let dp_i = dp.get(i).copied().unwrap_or(inf);
        if dp_i >= inf {
            continue;
        }
        // Windows clamp to `n`, so "reach the end in one edge" is always a
        // candidate: without it the pure powers of two could not express a
        // single span whose length is not itself a power of two, forcing the
        // DP to fragment even when *not* segmenting (the squeeze-equivalent) is
        // optimal. Clamping collapses the large windows onto `j = n`; skip the
        // duplicates it creates.
        let mut prev_j = i;
        for &w in &WINDOWS {
            let j = (i + w).min(n);
            if j <= prev_j {
                continue;
            }
            prev_j = j;
            let (Some(&start), Some(&end)) = (offsets.get(i), offsets.get(j)) else {
                continue;
            };
            let Some(span) = text.get(start..end) else {
                continue;
            };
            let (artifact, weight) = best_span(span, meter);
            let cost = dp_i.saturating_add(weight).saturating_add(FRAME_COST);
            if cost < dp.get(j).copied().unwrap_or(inf) {
                if let Some(slot) = dp.get_mut(j) {
                    *slot = cost;
                }
                if let Some(slot) = best.get_mut(j) {
                    *slot = Some((i, artifact));
                }
            }
        }
    }

    if dp.get(n).copied().unwrap_or(inf) >= inf {
        return None;
    }

    // Walk the back-pointers from `N` to `0` and reverse into forward order.
    let mut segs = Vec::new();
    let mut j = n;
    while j > 0 {
        let (i, artifact) = best.get_mut(j).and_then(Option::take)?;
        segs.push(artifact);
        j = i;
    }
    segs.reverse();
    Some(segs)
}

/// The cheapest byte-exact artifact for one span, and its measured token cost.
/// Every candidate already falls back to `raw` internally, so the raw floor is
/// always in the running and the measured minimum is safe to take.
fn best_span(span: &str, meter: &dyn TokenMeter) -> (String, usize) {
    let mut best = container::raw(span);
    let mut best_weight = meter.count(&best);
    for candidate in [
        fold::encode(span, meter),
        grep::encode(span, meter),
        diag::encode(span, meter),
        tmpl::encode(span, meter),
    ] {
        let weight = meter.count(&candidate);
        if weight < best_weight {
            best_weight = weight;
            best = candidate;
        }
    }
    (best, best_weight)
}

/// Wrap the per-region artifacts into a `%q1 mosaic` container: each segment is
/// framed by its decimal byte length on its own line, then the segment bytes
/// verbatim. Length framing means the segments need no separator and may
/// contain any bytes (including their own `%q1 body` lines).
pub fn emit(segs: &[String]) -> String {
    let mut body = String::new();
    for s in segs {
        body.push_str(&s.len().to_string());
        body.push('\n');
        body.push_str(s);
    }
    container::emit(&Container {
        codec: "mosaic".to_string(),
        params: vec![("n".to_string(), segs.len().to_string())],
        legend: Vec::new(),
        body,
    })
}

/// Split a `mosaic` container body back into its segment artifact strings,
/// checked against the header count and refusing trailing garbage. The caller
/// decodes each segment (one container layer) and concatenates.
pub fn split(c: &Container) -> Result<Vec<String>> {
    let n: usize = c
        .param("n")
        .unwrap_or("0")
        .parse()
        .context("mosaic: bad or missing n= segment count")?;
    let body = &c.body;
    let mut segs = Vec::with_capacity(n);
    let mut pos = 0usize;
    while pos < body.len() {
        let rest = body.get(pos..).unwrap_or_default();
        let nl = rest
            .find('\n')
            .context("mosaic: segment length line has no newline")?;
        let len_str = rest.get(..nl).unwrap_or_default();
        let len: usize = len_str
            .parse()
            .with_context(|| format!("mosaic: bad segment length {len_str:?}"))?;
        let seg_start = pos + nl + 1;
        let seg_end = seg_start.saturating_add(len);
        let seg = body
            .get(seg_start..seg_end)
            .with_context(|| format!("mosaic: segment length {len} overruns body"))?;
        segs.push(seg.to_string());
        pos = seg_end;
    }
    if segs.len() != n {
        bail!(
            "mosaic: header declares n={n} but body holds {} segment(s)",
            segs.len()
        );
    }
    Ok(segs)
}
