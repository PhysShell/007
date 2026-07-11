//! `rank` — a learned probe ranker for the miners.
//!
//! Every mining round measures whole-text token gain for each candidate it
//! probes; those (features, gain) pairs are free training data the lab has
//! been throwing away. This module keeps them as *sufficient statistics*
//! for ridge regression — `XᵀX` and `Xᵀy`, constant-size, deterministic to
//! accumulate and to merge across runs — inside the profile, and solves
//! them into linear weights that predict a candidate's gain from cheap
//! byte-level features (no tokenizer calls).
//!
//! The ranker never decides anything. It reorders the probe queue and
//! lets the miner spend its measured probes on the candidates most likely
//! to pay; every commit still passes the same whole-text measured
//! acceptance. A wrong model wastes probes, never bytes — the same
//! contract as profile seeds.

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

/// Feature dimension, bias included. Bump only with a profile-format note:
/// stats of different dimension refuse to merge.
pub const DIM: usize = 8;

/// Ridge regularizer: keeps the solve stable when features are collinear
/// on small samples (e.g. every candidate in one log shares a shape).
const LAMBDA: f64 = 1.0;
/// Below this many observations the solve is refused — a handful of
/// samples produces confident nonsense.
const MIN_SAMPLES: u64 = 64;

/// Cheap byte-level features of a probe candidate. `count` is the miner's
/// occurrence estimate for the span, `len`-independent signals cover the
/// shapes we mine (paths, identifiers, message stems).
pub fn features(phrase: &str, count: usize) -> [f64; DIM] {
    let len = phrase.len().max(1) as f64;
    let chars = phrase.chars().count().max(1) as f64;
    let words = phrase.split_whitespace().count() as f64;
    let seps = phrase
        .chars()
        .filter(|c| matches!(c, '/' | '\\' | '.' | ':'))
        .count() as f64;
    let digits = phrase.chars().filter(char::is_ascii_digit).count() as f64;
    let upper = phrase.chars().filter(|c| c.is_uppercase()).count() as f64;
    [
        1.0,                       // bias
        len.ln(),                  // size
        ((count.max(1)) as f64).ln(), // repetition
        words,                     // span width in words
        seps / chars,              // path/namespace density
        digits / chars,            // numeric fraction (volatile content)
        upper / chars,             // identifier casing
        f64::from(phrase.ends_with(['/', '\\', '.', ':'])), // segment prefix
    ]
}

/// Accumulated `XᵀX`/`Xᵀy` — everything ridge regression needs, nothing
/// that grows with sample count. Deterministic: accumulation is plain
/// summation in observation order, merging is matrix addition.
#[derive(Debug, Clone)]
pub struct Stats {
    xtx: Vec<f64>, // row-major DIM×DIM
    xty: Vec<f64>,
    pub n: u64,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            xtx: vec![0.0; DIM * DIM],
            xty: vec![0.0; DIM],
            n: 0,
        }
    }
}

impl Stats {
    pub fn observe(&mut self, x: &[f64; DIM], gain: f64) {
        for (i, &xi) in x.iter().enumerate() {
            if let Some(slot) = self.xty.get_mut(i) {
                *slot += xi * gain;
            }
            for (j, &xj) in x.iter().enumerate() {
                if let Some(slot) = self.xtx.get_mut(i * DIM + j) {
                    *slot += xi * xj;
                }
            }
        }
        self.n += 1;
    }

    pub fn merge(&mut self, other: &Stats) {
        for (a, b) in self.xtx.iter_mut().zip(&other.xtx) {
            *a += b;
        }
        for (a, b) in self.xty.iter_mut().zip(&other.xty) {
            *a += b;
        }
        self.n += other.n;
    }

    /// Solve `(XᵀX + λI) w = Xᵀy` by Gaussian elimination with partial
    /// pivoting. Refuses on too few samples or a degenerate system —
    /// callers fall back to the count×len heuristic.
    pub fn solve(&self) -> Option<Ranker> {
        if self.n < MIN_SAMPLES {
            return None;
        }
        // Augmented matrix [A | b], A = XᵀX + λI.
        let mut a: Vec<f64> = self.xtx.clone();
        for i in 0..DIM {
            if let Some(slot) = a.get_mut(i * DIM + i) {
                *slot += LAMBDA;
            }
        }
        let mut b: Vec<f64> = self.xty.clone();
        for col in 0..DIM {
            // Pivot: largest |value| in this column at or below the diagonal.
            let pivot = (col..DIM)
                .max_by(|&r1, &r2| {
                    let v1 = a.get(r1 * DIM + col).copied().unwrap_or(0.0).abs();
                    let v2 = a.get(r2 * DIM + col).copied().unwrap_or(0.0).abs();
                    v1.total_cmp(&v2)
                })
                .unwrap_or(col);
            let pivot_val = a.get(pivot * DIM + col).copied().unwrap_or(0.0);
            if pivot_val.abs() < 1e-9 {
                return None; // degenerate — refuse rather than extrapolate
            }
            if pivot != col {
                for k in 0..DIM {
                    let hi = a.get(pivot * DIM + k).copied().unwrap_or(0.0);
                    let lo = a.get(col * DIM + k).copied().unwrap_or(0.0);
                    if let Some(slot) = a.get_mut(pivot * DIM + k) {
                        *slot = lo;
                    }
                    if let Some(slot) = a.get_mut(col * DIM + k) {
                        *slot = hi;
                    }
                }
                let hi = b.get(pivot).copied().unwrap_or(0.0);
                let lo = b.get(col).copied().unwrap_or(0.0);
                if let Some(slot) = b.get_mut(pivot) {
                    *slot = lo;
                }
                if let Some(slot) = b.get_mut(col) {
                    *slot = hi;
                }
            }
            let diag = a.get(col * DIM + col).copied().unwrap_or(1.0);
            for row in (col + 1)..DIM {
                let factor = a.get(row * DIM + col).copied().unwrap_or(0.0) / diag;
                if factor == 0.0 {
                    continue;
                }
                for k in col..DIM {
                    let upper = a.get(col * DIM + k).copied().unwrap_or(0.0);
                    if let Some(slot) = a.get_mut(row * DIM + k) {
                        *slot -= factor * upper;
                    }
                }
                let upper_b = b.get(col).copied().unwrap_or(0.0);
                if let Some(slot) = b.get_mut(row) {
                    *slot -= factor * upper_b;
                }
            }
        }
        // Back-substitution.
        let mut w = vec![0.0f64; DIM];
        for col in (0..DIM).rev() {
            let mut acc = b.get(col).copied().unwrap_or(0.0);
            for k in (col + 1)..DIM {
                acc -= a.get(col * DIM + k).copied().unwrap_or(0.0)
                    * w.get(k).copied().unwrap_or(0.0);
            }
            let diag = a.get(col * DIM + col).copied().unwrap_or(1.0);
            if let Some(slot) = w.get_mut(col) {
                *slot = acc / diag;
            }
        }
        if w.iter().any(|v| !v.is_finite()) {
            return None;
        }
        Some(Ranker { weights: w })
    }

    pub fn to_json(&self) -> Value {
        json!({ "d": DIM, "n": self.n, "xtx": self.xtx, "xty": self.xty })
    }

    pub fn from_json(v: &Value) -> Result<Self> {
        let d = v.get("d").and_then(Value::as_u64).unwrap_or(0) as usize;
        if d != DIM {
            bail!("ranker stats dimension {d} != {DIM} (feature set changed)");
        }
        let read = |key: &str, want: usize| -> Result<Vec<f64>> {
            let arr = v
                .get(key)
                .and_then(Value::as_array)
                .with_context(|| format!("ranker stats missing {key}"))?;
            let out: Option<Vec<f64>> = arr.iter().map(Value::as_f64).collect();
            let out = out.with_context(|| format!("non-numeric value in {key}"))?;
            if out.len() != want {
                bail!("{key} has {} entries, want {want}", out.len());
            }
            Ok(out)
        };
        Ok(Self {
            xtx: read("xtx", DIM * DIM)?,
            xty: read("xty", DIM)?,
            n: v.get("n").and_then(Value::as_u64).unwrap_or(0),
        })
    }
}

/// Fitted linear weights: score = w · features. Higher is better.
#[derive(Debug, Clone)]
pub struct Ranker {
    weights: Vec<f64>,
}

impl Ranker {
    pub fn score(&self, x: &[f64; DIM]) -> f64 {
        self.weights
            .iter()
            .zip(x.iter())
            .map(|(w, xi)| w * xi)
            .sum()
    }
}
