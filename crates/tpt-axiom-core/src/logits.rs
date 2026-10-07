//! Logit-space primitives for model outputs: numerically stable softmax,
//! cross-entropy, and top-k selection.
//!
//! These are the mathematical core behind `tpt-axiom-interop`'s
//! [`EngineOutput::from_logits`](https://docs.rs/tpt-axiom-interop) — placed
//! in `core` so pipelines that produce logits directly (not through the
//! engine boundary) share one tested implementation.

// Without std, the f64 float methods come from num-traits (libm).
use alloc::vec::Vec;
#[allow(unused_imports)]
use num_traits::Float as _;

/// Numerically stable temperature softmax over logits.
///
/// The max logit is subtracted before exponentiation, so arbitrarily large
/// finite logits cannot overflow; every returned probability is finite and
/// the vector sums to one (up to rounding). `temperature` must be finite
/// and positive — below one sharpens, above one flattens.
///
/// # Panics
/// Panics if `temperature` is non-finite or non-positive (a policy error,
/// not a per-element one).
#[must_use]
pub fn softmax(logits: &[f64], temperature: f64) -> Vec<f64> {
    assert!(
        temperature.is_finite() && temperature > 0.0,
        "softmax temperature must be finite and positive, got {temperature}"
    );
    let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let scaled: Vec<f64> = logits.iter().map(|&l| (l - max) / temperature).collect();
    let total: f64 = scaled.iter().map(|&s| s.exp()).sum();
    scaled.iter().map(|&s| (s.exp()) / total).collect()
}

/// Log-softmax (cross-entropy building block): stable log probabilities.
///
/// # Panics
/// Panics if `temperature` is non-finite or non-positive.
#[must_use]
pub fn log_softmax(logits: &[f64], temperature: f64) -> Vec<f64> {
    assert!(
        temperature.is_finite() && temperature > 0.0,
        "softmax temperature must be finite and positive, got {temperature}"
    );
    let max = logits.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let shifted: Vec<f64> = logits.iter().map(|&l| (l - max) / temperature).collect();
    let log_total: f64 = shifted.iter().map(|&s| s.exp()).sum::<f64>().ln();
    shifted.iter().map(|&s| s - log_total).collect()
}

/// Cross-entropy loss of `target` under logits: `−log softmax(logits)[target]`.
///
/// # Panics
/// Panics if `target` is out of bounds or the temperature is invalid.
#[must_use]
pub fn cross_entropy(logits: &[f64], target: usize, temperature: f64) -> f64 {
    assert!(target < logits.len(), "target {target} out of bounds");
    -log_softmax(logits, temperature)[target]
}

/// The `k` highest logits as `(index, logit)` pairs, best first (stable for
/// ties). `k` is clamped to `logits.len()`.
#[must_use]
pub fn top_k(logits: &[f64], k: usize) -> Vec<(usize, f64)> {
    let mut indexed: Vec<(usize, f64)> = logits.iter().copied().enumerate().collect();
    indexed.sort_by(|a, b| b.1.total_cmp(&a.1));
    indexed.truncate(k.min(logits.len()));
    indexed
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // exact invariants on hand-computed values
    use super::*;
    use alloc::vec;

    #[test]
    fn softmax_normalizes_and_handles_extremes() {
        let s = softmax(&[0.0, 0.0], 1.0);
        assert_eq!(s, vec![0.5, 0.5]);
        // Arbitrarily large finite logits cannot overflow.
        let s = softmax(&[1.0e300, 1.0e300, 0.0], 1.0);
        assert!(close(s[0], 0.5, 1e-12) && close(s[1], 0.5, 1e-12) && s[2] < 1e-250);
        // Temperature sharpening: T → 0⁺ approaches argmax; T large flattens.
        let sharp = softmax(&[1.0, 2.0], 0.01);
        assert!(sharp[1] > 0.99);
        let flat = softmax(&[1.0, 2.0], 1.0e6);
        assert!(close(flat[0], 0.5, 1e-5));
    }

    #[test]
    #[should_panic(expected = "softmax temperature")]
    fn softmax_rejects_bad_temperature() {
        let _ = softmax(&[1.0, 2.0], 0.0);
    }

    #[test]
    fn cross_entropy_matches_log_softmax() {
        let logits = [1.0, 2.0, 3.0];
        let ls = log_softmax(&logits, 1.0);
        // log softmax sums to log(1) = 0 in exponentiated space.
        let total: f64 = ls.iter().map(|&l| l.exp()).sum();
        assert!(close(total, 1.0, 1e-12));
        // Loss of the true class under a one-hot target.
        let loss = cross_entropy(&logits, 2, 1.0);
        assert!(close(loss, -ls[2], 1e-15));
        assert!(loss > 0.0);
    }

    #[test]
    fn top_k_selects_best_first() {
        let k = top_k(&[0.3, 9.0, -1.0, 4.0], 2);
        assert_eq!(k, vec![(1, 9.0), (3, 4.0)]);
        // k beyond the length is clamped.
        assert_eq!(top_k(&[3.0, 1.0], 10).len(), 2);
        assert!(top_k(&[], 3).is_empty());
    }

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }
}
