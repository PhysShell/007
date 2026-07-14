//! `mosaic` — measured segmentation across codecs.
//!
//! Where `squeeze` picks *one* structural codec for the whole payload,
//! `mosaic` cuts the payload at line boundaries and routes each region to the
//! codec that measures cheapest *there* — the diagnostic block to `diag`, a
//! repetitive run to `fold`, unique prose to `raw` — then (in `lib`) mines the
//! whole assembled artifact. The split is a shortest path over a DAG of span
//! candidates: a node is a boundary between lines, an edge `i -> j` is the
//! region `[i, j)` encoded by one codec, weighted by the *measured* full token
//! cost of that nested artifact (header, legend and all).
//!
//! ## Two candidate graphs, and the honest limits of each
//!
//! The production router ([`encode_seeded`]) uses a **geometric** candidate
//! graph: from each start it tries window sizes `1,2,4,8,16,32,64,128` lines,
//! plus an explicit whole-payload edge. This is `O(N·W)` and fast, but it is
//! *not* the optimal segmentation — a beneficial region whose length is not on
//! the grid (a 45-line block in the middle of a 500-line file) can only be
//! spelled `32+8+4+1`, paying four headers. So it answers the narrow question
//! "is there a win among geometric spans?", not "is there a win at all?".
//!
//! [`all_span_dp`] widens the graph to **every** span `[i, j)`, `O(N²)` of
//! them — run offline on small payloads for the kill criterion. But note what
//! it is *not*: the path is still chosen by the **additive** edge model
//! (`dp[i] + meter.count(edge) + frame`), and because BPE is not additive
//! (`tok(A+B) != tok(A) + tok(B)`) with a `frame` that only approximates the
//! real length-prefix + envelope cost, the DP-selected path is then the *only*
//! multi-segment artifact measured exactly (against the whole-span baseline).
//! It is an exhaustive-span *additive DP*, not a token-exact oracle: it can
//! prove "the all-span additive DP found no segmentation the exact meter
//! prefers over not-segmenting", which is a strong negative — but it is not a
//! mathematical minimum over all assembled artifacts. [`AllSpanReport`] exposes
//! the DP's *pre-arbitration* choice so a test can check what the DP actually
//! picked, not just the baseline-clamped result.
//!
//! Either way, [`encode_seeded`] measures the assembled path against the
//! whole-payload baseline with the exact meter and keeps the real minimum — so
//! an approximate edge model can waste probes but cannot ship a path the exact
//! meter rejects.
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
//! length, takes exactly that many bytes, decodes one layer, and concatenates.
//! Byte-exact: the segments partition the input, every candidate here is itself
//! byte-exact, and a single-segment result is emitted *bare* (identity elision
//! — no `%q1 mosaic` wrapper), exactly as `squeeze` never wraps its winner.
//!
//! Deliberately deferred: `toon` segments (semantic, not byte-exact), per-span
//! `mine`/`deep`, a shared opcode table stripping the repeated `%q1` headers,
//! and a top-K path search (v1 measures the one DP path against the whole-span
//! baseline only — cheap insurance against BPE non-additivity, not worth the
//! code until a real multi-segment winner exists to protect).

use anyhow::{bail, Context, Result};

use crate::container::{self, Container};
use crate::meter::TokenMeter;
use crate::{diag, fold, grep, tmpl};

/// Above this many lines the geometric `O(N·W)` sweep is not worth it for a lab
/// codec; fall back to a single raw container (the caller still measures it).
const MAX_LINES: usize = 4000;

/// The exhaustive [`all_span_dp`] is `O(N²)` spans × structural codecs; keep it
/// to small payloads where truth is cheap enough. Larger inputs return `None`.
const MAX_ALL_SPAN_LINES: usize = 300;

/// Geometric window sizes (in lines) tried from every start in the production
/// router. `1` guarantees the DAG is always connected; the larger sizes let a
/// region grow until a codec has enough lines to pay for its own header.
const WINDOWS: [usize; 8] = [1, 2, 4, 8, 16, 32, 64, 128];

/// A small nudge per extra segment: the length-prefix line, plus a tie-breaker
/// toward fewer, larger spans. The real per-segment cost — the nested `%q1`
/// header — already rides inside each edge weight; this only breaks ties.
const FRAME_COST: usize = 1;

/// Hard cap on the segment count a decoder will honour from an untrusted
/// header, before any allocation sized by it.
const MAX_SEGMENTS: usize = 4096;

/// Encode `text` with no profile seeds — see [`encode_seeded`].
pub fn encode(text: &str, meter: &dyn TokenMeter) -> String {
    encode_seeded(text, meter, &[])
}

/// Encode `text` via the geometric router, then keep whichever is cheaper by
/// the exact meter: the routed (possibly segmented) artifact or the
/// whole-payload single-codec baseline. `templates` are the profile-learned
/// `tmpl` seeds — threaded into every per-span `tmpl` candidate so the routing
/// stage sees the same clusters `squeeze` does (without them, a learned profile
/// would change `squeeze`'s candidates but not mosaic's, and the two would
/// stop being comparable). Falls back to a single raw container when the input
/// is empty or larger than `MAX_LINES`.
pub fn encode_seeded(text: &str, meter: &dyn TokenMeter, templates: &[Vec<String>]) -> String {
    let path = segment(text, meter, false, templates);
    routed_or_baseline(text, meter, path, templates)
}

/// The pre-arbitration verdict of the all-span additive DP — see
/// [`all_span_dp`]. Fields separate what the DP *chose* from what baseline
/// arbitration would then keep, so a caller can tell "the DP itself declined to
/// split" from "the DP split but the baseline was silently substituted".
#[derive(Debug, Clone)]
pub struct AllSpanReport {
    /// The assembled artifact of the DP-selected path, *before* baseline
    /// arbitration — bare when the DP chose a single segment.
    pub artifact: String,
    /// Segments in the DP-selected path (`1` = the DP itself did not split).
    pub segments: usize,
    /// Sum of edge weights along the DP path — the additive model's own cost.
    pub additive_cost: usize,
    /// Exact `meter.count` of the assembled DP artifact.
    pub exact_tokens: usize,
    /// Exact `meter.count` of the whole-payload single-codec baseline.
    pub baseline_tokens: usize,
}

/// Run the **all-span additive DP** (every span `[i, j)`, not the geometric
/// grid) and report its pre-arbitration choice. This is the kill-criterion
/// truth-teller, bounded to small payloads (`MAX_ALL_SPAN_LINES`); `None` when
/// the input is too large or unsegmentable.
///
/// It is *not* a token-exact oracle: the path is selected by the additive edge
/// model, so it establishes "no all-span additive-DP path, exactly measured,
/// beats the whole-span baseline", not a proven global minimum. The returned
/// [`AllSpanReport::artifact`] is the DP's own choice, un-arbitrated.
pub fn all_span_dp(
    text: &str,
    meter: &dyn TokenMeter,
    templates: &[Vec<String>],
) -> Option<AllSpanReport> {
    let lines = line_count(text)?;
    if lines > MAX_ALL_SPAN_LINES {
        return None;
    }
    let segs = segment(text, meter, true, templates)?;
    let additive_cost = segs
        .iter()
        .map(|s| meter.count(s).saturating_add(FRAME_COST))
        .sum();
    let artifact = assemble(&segs);
    Some(AllSpanReport {
        exact_tokens: meter.count(&artifact),
        baseline_tokens: meter.count(&best_span(text, meter, templates).0),
        segments: segs.len(),
        additive_cost,
        artifact,
    })
}

/// Number of line units, or `None` if the input is empty / over `MAX_LINES`.
fn line_count(text: &str) -> Option<usize> {
    if text.is_empty() {
        return None;
    }
    let n = text.split_inclusive('\n').count();
    (n > 0 && n <= MAX_LINES).then_some(n)
}

/// Given a candidate path (or `None`), assemble it and return the exact-meter
/// minimum of {assembled path, whole-payload single-codec baseline}. This is
/// the guarantee point-3 needs: the additive DP can misrank a multi-segment
/// path that the exact meter then rejects, so "not segmenting" is always a
/// measured competitor, never assumed away.
fn routed_or_baseline(
    text: &str,
    meter: &dyn TokenMeter,
    path: Option<Vec<String>>,
    templates: &[Vec<String>],
) -> String {
    let baseline = best_span(text, meter, templates).0;
    match path {
        Some(segs) => {
            let candidate = assemble(&segs);
            if meter.count(&candidate) <= meter.count(&baseline) {
                candidate
            } else {
                baseline
            }
        }
        None => baseline,
    }
}

/// Assemble a chosen path. A single segment is returned *bare* — no envelope,
/// so a routed result that declines to split costs exactly what the plain
/// codec costs, with no self-inflicted container tax.
fn assemble(segs: &[String]) -> String {
    match segs {
        [single] => single.clone(),
        _ => emit(segs),
    }
}

/// The shortest-path segmentation: the ordered per-region artifacts of the
/// cheapest `0..N` path. `exhaustive` selects the candidate graph (every span
/// vs the geometric grid); `templates` seed every per-span `tmpl` candidate.
fn segment(
    text: &str,
    meter: &dyn TokenMeter,
    exhaustive: bool,
    templates: &[Vec<String>],
) -> Option<Vec<String>> {
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
        for j in candidate_ends(i, n, exhaustive) {
            let (Some(&start), Some(&end)) = (offsets.get(i), offsets.get(j)) else {
                continue;
            };
            let Some(span) = text.get(start..end) else {
                continue;
            };
            let (artifact, weight) = best_span(span, meter, templates);
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

/// The set of end boundaries considered from start `i`.
///
/// * exhaustive: every `j` in `i+1..=n` — the true candidate graph.
/// * geometric: the clamped power-of-two windows, **plus an explicit
///   whole-payload edge** so "don't segment" is a real candidate at *any* `N`
///   (the clamp alone only reaches `n` when `n - i <= 128`, which is exactly
///   the flaw the review caught — a 500-line file's `0 -> n` edge is otherwise
///   absent). Reaching the end from an arbitrary mid-file start is *not* in the
///   geometric grid; that is what [`all_span_dp`] is for.
fn candidate_ends(i: usize, n: usize, exhaustive: bool) -> Vec<usize> {
    if exhaustive {
        return ((i + 1)..=n).collect();
    }
    let mut ends = Vec::with_capacity(WINDOWS.len() + 1);
    let mut prev = i;
    for &w in &WINDOWS {
        let j = (i + w).min(n);
        if j > prev {
            ends.push(j);
            prev = j;
        }
    }
    if i == 0 && n > prev {
        ends.push(n); // the whole-payload baseline edge
    }
    ends
}

/// The cheapest byte-exact artifact for one span, and its measured token cost.
/// Every candidate already falls back to `raw` internally, so the raw floor is
/// always in the running and the measured minimum is safe to take. `tmpl` is
/// seeded with the profile `templates` so the routing stage clusters exactly as
/// `squeeze` would.
fn best_span(span: &str, meter: &dyn TokenMeter, templates: &[Vec<String>]) -> (String, usize) {
    let mut best = container::raw(span);
    let mut best_weight = meter.count(&best);
    for candidate in [
        fold::encode(span, meter),
        grep::encode(span, meter),
        diag::encode(span, meter),
        tmpl::encode_seeded(span, meter, templates),
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
/// verbatim. Length framing means segments need no separator and may contain
/// any bytes (including their own `%q1 body` lines).
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
///
/// `n` comes from an untrusted header, so it is bounded before it sizes any
/// allocation: a segment needs at least a length line and a byte, so `n` can
/// never exceed `body.len()`, and never `MAX_SEGMENTS` regardless.
pub fn split(c: &Container) -> Result<Vec<String>> {
    let n: usize = c
        .param("n")
        .unwrap_or("0")
        .parse()
        .context("mosaic: bad or missing n= segment count")?;
    let body = &c.body;
    if n > MAX_SEGMENTS || n > body.len().saturating_add(1) {
        bail!(
            "mosaic: unreasonable segment count n={n} for a {}-byte body",
            body.len()
        );
    }
    let mut segs = Vec::with_capacity(n);
    let mut pos = 0usize;
    while pos < body.len() {
        if segs.len() > n {
            bail!("mosaic: body holds more than the declared n={n} segments");
        }
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
