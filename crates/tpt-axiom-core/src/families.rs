//! Named distribution families with exact closed-form moments and
//! dependency-free `pdf`/`cdf`/`quantile`.
//!
//! [`Distribution<T>`](crate::Distribution) is the Gaussian-shaped summary
//! the uncertainty arithmetic operates on; these families are the *inputs*
//! to that arithmetic — measurement models, likelihoods, priors — where the
//! shape matters. Every family carries:
//!
//! * validated construction (fallible `new`, out-of-domain parameters are
//!   rejected, never clamped);
//! * exact `mean`/`variance` where closed forms exist;
//! * `pdf`/`cdf`/`quantile` computed through the crate's own
//!   [`special`](crate::special) functions (Lanczos lgamma, incomplete
//!   gamma/beta) — accurate to ~1e-13, no external dependencies;
//! * [`ContinuousDistribution::prob_greater_than`] as a named survival
//!   query, and conjugate-update constructors ([`Beta::update_bernoulli`],
//!   [`Gamma::update_poisson`]) for the two classic textbook cases.
//!
//! Converting a family into the uncertainty arithmetic is
//! `Fuzzy::new(d.mean(), d.variance())` or
//! `d.moments_approx()` — the `_approx` name carries the same honesty rule
//! as the Augur bridge: only the first two moments survive.

// The structural allowances below are deliberate choices: f64-backed values
// are not `Eq`; parameter checks compare against exact constants; counts
// feed moment formulas that tolerate 2^53.
#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::float_cmp)]

// Without std, the f64 float methods come from num-traits (libm).
use crate::special::{beta_reg, gamma_p, gamma_q, lgamma};
use core::f64::consts::PI;
use core::fmt;
#[cfg(not(feature = "std"))]
use num_traits::Float as _;

/// A parameter was outside the domain its family's math requires.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DomainError {
    /// Which family rejected the parameters.
    pub family: &'static str,
    /// The human-readable domain violation.
    pub reason: &'static str,
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.family, self.reason)
    }
}

impl core::error::Error for DomainError {}

fn require(family: &'static str, ok: bool, reason: &'static str) -> Result<(), DomainError> {
    if ok {
        Ok(())
    } else {
        Err(DomainError { family, reason })
    }
}

/// A continuous distribution over the reals: density, CDF, and quantile.
pub trait ContinuousDistribution {
    /// The distribution's mean (may be undefined — NaN — for some parameter
    /// ranges, e.g. Student-t with ν ≤ 1).
    fn mean(&self) -> f64;
    /// The distribution's variance (may be undefined — NaN or infinite).
    fn variance(&self) -> f64;
    /// The density at `x`.
    fn pdf(&self, x: f64) -> f64;
    /// `P(X ≤ x)`.
    fn cdf(&self, x: f64) -> f64;
    /// The inverse CDF: the quantile `x` with `P(X ≤ x) = p`.
    ///
    /// # Panics
    /// Panics if `p` is outside `(0, 1)`.
    fn quantile(&self, p: f64) -> f64;
    /// `P(X > x)` — the survival function. Default: `1 − cdf(x)`; families
    /// with an accurate far-tail form override it.
    fn prob_greater_than(&self, x: f64) -> f64 {
        1.0 - self.cdf(x)
    }

    /// The `(mean, variance)` pair, ready for the uncertainty arithmetic
    /// (`Fuzzy::new(m, v)` / `Uncertain::estimated(m, v)`). Only the first
    /// two moments survive that conversion — the same honesty rule as the
    /// Augur bridge's `_approx` conversions.
    fn moments(&self) -> (f64, f64) {
        (self.mean(), self.variance())
    }

    /// The central credible/coverage interval: the `(quantile(tail),
    /// quantile(1 − tail))` pair containing approximately `level` of the
    /// mass.
    ///
    /// # Panics
    /// Panics if `level` is outside `(0, 1)`.
    fn credible_interval(&self, level: f64) -> (f64, f64) {
        assert!(
            level > 0.0 && level < 1.0,
            "credible interval needs 0 < level < 1, got {level}"
        );
        let tail = (1.0 - level) / 2.0;
        (self.quantile(tail), self.quantile(1.0 - tail))
    }
}

/// A discrete distribution over the non-negative integers.
pub trait DiscreteDistribution {
    /// The probability mass at `k`.
    fn pmf(&self, k: u64) -> f64;
    /// `P(X ≤ k)`.
    fn cdf(&self, k: u64) -> f64;
    /// The mean.
    fn mean(&self) -> f64;
    /// The variance.
    fn variance(&self) -> f64;
}

/// Uniform distribution on `[lo, hi]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Uniform {
    lo: f64,
    hi: f64,
}

impl Uniform {
    /// A uniform distribution on `[lo, hi]`.
    ///
    /// # Errors
    /// [`DomainError`] when `lo`/`hi` are non-finite or `hi <= lo`.
    pub fn new(lo: f64, hi: f64) -> Result<Self, DomainError> {
        require(
            "Uniform",
            lo.is_finite() && hi.is_finite() && hi > lo,
            "needs finite bounds with hi > lo",
        )?;
        Ok(Self { lo, hi })
    }

    /// The lower bound.
    #[must_use]
    pub const fn lo(&self) -> f64 {
        self.lo
    }

    /// The upper bound.
    #[must_use]
    pub const fn hi(&self) -> f64 {
        self.hi
    }
}

impl ContinuousDistribution for Uniform {
    fn mean(&self) -> f64 {
        self.lo + (self.hi - self.lo) / 2.0
    }
    fn variance(&self) -> f64 {
        let w = self.hi - self.lo;
        w * w / 12.0
    }
    fn pdf(&self, x: f64) -> f64 {
        if (self.lo..=self.hi).contains(&x) {
            1.0 / (self.hi - self.lo)
        } else {
            0.0
        }
    }
    fn cdf(&self, x: f64) -> f64 {
        if x <= self.lo {
            0.0
        } else if x >= self.hi {
            1.0
        } else {
            (x - self.lo) / (self.hi - self.lo)
        }
    }
    fn quantile(&self, p: f64) -> f64 {
        assert!((0.0..=1.0).contains(&p), "quantile needs p in [0, 1]");
        self.lo + p * (self.hi - self.lo)
    }
}

/// Beta distribution on `(0, 1)` with both shape parameters positive — the
/// conjugate prior for a Bernoulli probability.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Beta {
    a: f64,
    b: f64,
}

impl Beta {
    /// A Beta distribution with shapes `a`, `b > 0`.
    ///
    /// # Errors
    /// [`DomainError`] when a shape is non-finite or non-positive.
    pub fn new(a: f64, b: f64) -> Result<Self, DomainError> {
        require(
            "Beta",
            a.is_finite() && b.is_finite() && a > 0.0 && b > 0.0,
            "shapes must be finite and positive",
        )?;
        Ok(Self { a, b })
    }

    /// The `a` shape.
    #[must_use]
    pub const fn a(&self) -> f64 {
        self.a
    }

    /// The `b` shape.
    #[must_use]
    pub const fn b(&self) -> f64 {
        self.b
    }

    /// The Beta–Bernoulli conjugate update: `Beta(a + successes, b + failures)`.
    ///
    /// `successes`/`failures` are counts; negative counts are treated as 0.
    #[must_use]
    pub fn update_bernoulli(&self, successes: u64, failures: u64) -> Self {
        Self {
            a: self.a + successes as f64,
            b: self.b + failures as f64,
        }
    }
}

impl ContinuousDistribution for Beta {
    fn mean(&self) -> f64 {
        self.a / (self.a + self.b)
    }
    fn variance(&self) -> f64 {
        let s = self.a + self.b;
        self.a * self.b / (s * s * (s + 1.0))
    }
    fn pdf(&self, x: f64) -> f64 {
        if !(0.0..=1.0).contains(&x) {
            return 0.0;
        }
        // Boundary limits of x^{a−1}(1−x)^{b−1}/B(a,b).
        if x == 0.0 {
            return if self.a < 1.0 {
                f64::INFINITY
            } else if self.a == 1.0 {
                self.b
            } else {
                0.0
            };
        }
        if x == 1.0 {
            return if self.b < 1.0 {
                f64::INFINITY
            } else if self.b == 1.0 {
                self.a
            } else {
                0.0
            };
        }
        let ln = (self.a - 1.0) * x.ln() + (self.b - 1.0) * (1.0 - x).ln()
            - crate::special::ln_beta(self.a, self.b);
        ln.exp()
    }
    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else if x >= 1.0 {
            1.0
        } else {
            beta_reg(self.a, self.b, x)
        }
    }
    fn quantile(&self, p: f64) -> f64 {
        assert!(p > 0.0 && p < 1.0, "quantile needs 0 < p < 1");
        bisect(|x| self.cdf(x), p, 0.0, 1.0)
    }
    fn prob_greater_than(&self, x: f64) -> f64 {
        if x <= 0.0 {
            1.0
        } else if x >= 1.0 {
            0.0
        } else {
            // Symmetry keeps the upper tail accurate.
            beta_reg(self.b, self.a, 1.0 - x)
        }
    }
}

/// Gamma distribution with shape `k > 0` and **rate** `rate > 0`
/// (mean `k/rate`), the conjugate prior for a Poisson rate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gamma {
    shape: f64,
    rate: f64,
}

impl Gamma {
    /// A Gamma distribution with shape `shape` and rate `rate`, both finite
    /// and positive.
    ///
    /// # Errors
    /// [`DomainError`] when a parameter is non-finite or non-positive.
    pub fn new(shape: f64, rate: f64) -> Result<Self, DomainError> {
        require(
            "Gamma",
            shape.is_finite() && rate.is_finite() && shape > 0.0 && rate > 0.0,
            "shape and rate must be finite and positive",
        )?;
        Ok(Self { shape, rate })
    }

    /// The shape parameter.
    #[must_use]
    pub const fn shape(&self) -> f64 {
        self.shape
    }

    /// The rate parameter (inverse scale).
    #[must_use]
    pub const fn rate(&self) -> f64 {
        self.rate
    }

    /// The Gamma–Poisson conjugate update: observing `total` events across
    /// `count` Poisson draws gives `Gamma(k + total, rate + count)`.
    #[must_use]
    pub fn update_poisson(&self, total: u64, count: u64) -> Self {
        Self {
            shape: self.shape + total as f64,
            rate: self.rate + count as f64,
        }
    }
}

impl ContinuousDistribution for Gamma {
    fn mean(&self) -> f64 {
        self.shape / self.rate
    }
    fn variance(&self) -> f64 {
        self.shape / (self.rate * self.rate)
    }
    fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            return 0.0;
        }
        let k = self.shape;
        let r = self.rate;
        (k * r.ln() + (k - 1.0) * x.ln() - r * x - lgamma(k)).exp()
    }
    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else {
            gamma_p(self.shape, self.rate * x)
        }
    }
    fn quantile(&self, p: f64) -> f64 {
        assert!(p > 0.0 && p < 1.0, "quantile needs 0 < p < 1");
        // Start from an exponential/normal-mixture rough guess, then bisect
        // with an exponentially widening upper bracket.
        let mean = self.mean();
        let mut hi = mean.max(1.0 / self.rate);
        // Bracket widening: the loop drives `cdf(hi)` up towards `p`, it is not
        // an equality test on floats (the comparison is monotone and bounded).
        #[allow(clippy::while_float)]
        while self.cdf(hi) < p {
            hi *= 2.0;
        }
        bisect(|x| self.cdf(x), p, 0.0, hi)
    }
    fn prob_greater_than(&self, x: f64) -> f64 {
        if x <= 0.0 {
            1.0
        } else {
            gamma_q(self.shape, self.rate * x)
        }
    }
}

/// Log-normal distribution: `exp(Normal(mu, sigma^2))`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogNormal {
    /// Mean of the *underlying* normal (in log space).
    mu: f64,
    /// Variance of the underlying normal (in log space).
    sigma2: f64,
}

impl LogNormal {
    /// A log-normal with underlying-normal mean `mu` and variance `sigma2`.
    ///
    /// # Errors
    /// [`DomainError`] when `sigma2` is negative or NaN.
    pub fn new(mu: f64, sigma2: f64) -> Result<Self, DomainError> {
        require(
            "LogNormal",
            sigma2.is_finite() && sigma2 >= 0.0,
            "sigma^2 must be finite and non-negative",
        )?;
        Ok(Self { mu, sigma2 })
    }

    /// The median: `e^mu` (the log-space mean, unchanged by the transform).
    #[must_use]
    pub fn median(&self) -> f64 {
        self.mu.exp()
    }
}

impl ContinuousDistribution for LogNormal {
    fn mean(&self) -> f64 {
        (self.mu + self.sigma2 / 2.0).exp()
    }
    fn variance(&self) -> f64 {
        let v = self.sigma2;
        v.exp_m1() * (2.0 * self.mu + v).exp()
    }
    fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 || self.sigma2 == 0.0 {
            return 0.0;
        }
        let s = self.sigma2.sqrt();
        let z = (x.ln() - self.mu) / s;
        (-z * z / 2.0).exp() / (x * s * (2.0 * PI).sqrt())
    }
    fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else {
            crate::quants::norm_cdf((x.ln() - self.mu) / self.sigma2.sqrt())
        }
    }
    fn quantile(&self, p: f64) -> f64 {
        assert!(p > 0.0 && p < 1.0, "quantile needs 0 < p < 1");
        (self.mu + self.sigma2.sqrt() * crate::quants::norm_ppf(p)).exp()
    }
    fn prob_greater_than(&self, x: f64) -> f64 {
        if x <= 0.0 {
            1.0
        } else {
            crate::quants::norm_cdf((self.mu - x.ln()) / self.sigma2.sqrt())
        }
    }
}

/// Student's t distribution with `dof > 0` degrees of freedom (standard
/// form: location 0, scale 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StudentT {
    dof: f64,
}

impl StudentT {
    /// A standard Student-t with `dof` degrees of freedom.
    ///
    /// # Errors
    /// [`DomainError`] when `dof` is non-finite or non-positive.
    pub fn new(dof: f64) -> Result<Self, DomainError> {
        require(
            "StudentT",
            dof.is_finite() && dof > 0.0,
            "degrees of freedom must be finite and positive",
        )?;
        Ok(Self { dof })
    }

    /// The degrees of freedom.
    #[must_use]
    pub const fn dof(&self) -> f64 {
        self.dof
    }
}

impl ContinuousDistribution for StudentT {
    fn mean(&self) -> f64 {
        if self.dof > 1.0 { 0.0 } else { f64::NAN }
    }
    fn variance(&self) -> f64 {
        if self.dof > 2.0 {
            self.dof / (self.dof - 2.0)
        } else {
            f64::INFINITY
        }
    }
    fn pdf(&self, x: f64) -> f64 {
        let v = self.dof;
        let ln = lgamma(f64::midpoint(v, 1.0))
            - lgamma(v / 2.0)
            - (v * PI).sqrt().ln()
            - f64::midpoint(v, 1.0) * (1.0 + x * x / v).ln();
        ln.exp()
    }
    fn cdf(&self, x: f64) -> f64 {
        let v = self.dof;
        if x == 0.0 {
            return 0.5;
        }
        let upper = beta_reg(v / 2.0, 0.5, v / (x * x + v));
        if x > 0.0 {
            1.0 - upper / 2.0
        } else {
            upper / 2.0
        }
    }
    fn quantile(&self, p: f64) -> f64 {
        assert!(p > 0.0 && p < 1.0, "quantile needs 0 < p < 1");
        // Heavy tails: the upper bracket must widen until it covers p.
        if p < 0.5 {
            -self.quantile(1.0 - p)
        } else {
            let mut hi = 1.0;
            // Bracket widening: the loop drives `cdf(hi)` up towards `p`, it is
            // not an equality test on floats (the comparison is monotone).
            #[allow(clippy::while_float)]
            while self.cdf(hi) < p {
                hi *= 2.0;
            }
            bisect(|x| self.cdf(x), p, 0.0, hi)
        }
    }
    fn prob_greater_than(&self, x: f64) -> f64 {
        // 1 − cdf computed through the (symmetric) beta form: for x > 0 the
        // upper tail *is* the beta expression, so no cancellation.
        let v = self.dof;
        if x == 0.0 {
            return 0.5;
        }
        let upper = beta_reg(v / 2.0, 0.5, v / (x * x + v));
        if x > 0.0 {
            upper / 2.0
        } else {
            1.0 - upper / 2.0
        }
    }
}

/// Poisson distribution over non-negative integers with rate `rate ≥ 0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Poisson {
    rate: f64,
}

impl Poisson {
    /// A Poisson distribution with rate `rate`.
    ///
    /// # Errors
    /// [`DomainError`] when `rate` is non-finite or negative.
    pub fn new(rate: f64) -> Result<Self, DomainError> {
        require(
            "Poisson",
            rate.is_finite() && rate >= 0.0,
            "rate must be finite and non-negative",
        )?;
        Ok(Self { rate })
    }

    /// The rate parameter.
    #[must_use]
    pub const fn rate(&self) -> f64 {
        self.rate
    }
}

impl DiscreteDistribution for Poisson {
    fn pmf(&self, k: u64) -> f64 {
        if self.rate == 0.0 {
            return if k == 0 { 1.0 } else { 0.0 };
        }
        (k as f64 * self.rate.ln() - self.rate - lgamma(k as f64 + 1.0)).exp()
    }
    fn cdf(&self, k: u64) -> f64 {
        // P(X ≤ k) = Q(k + 1, λ) — the *upper* regularized gamma, accurate
        // in both tails.
        if self.rate == 0.0 {
            return 1.0;
        }
        gamma_q(k as f64 + 1.0, self.rate)
    }
    fn mean(&self) -> f64 {
        self.rate
    }
    fn variance(&self) -> f64 {
        self.rate
    }
}

/// Binomial distribution over `{0, .., n}` with success chance `p`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Binomial {
    n: u64,
    p: f64,
}

impl Binomial {
    /// A binomial with `n` trials and chance `p ∈ [0, 1]`.
    ///
    /// # Errors
    /// [`DomainError`] when `p` is outside `[0, 1]` or NaN.
    pub fn new(n: u64, p: f64) -> Result<Self, DomainError> {
        require("Binomial", (0.0..=1.0).contains(&p), "p must lie in [0, 1]")?;
        Ok(Self { n, p })
    }

    /// The trial count.
    #[must_use]
    pub const fn n(&self) -> u64 {
        self.n
    }

    /// The success chance.
    #[must_use]
    pub const fn p(&self) -> f64 {
        self.p
    }
}

impl DiscreteDistribution for Binomial {
    fn pmf(&self, k: u64) -> f64 {
        if k > self.n {
            return 0.0;
        }
        if self.p == 0.0 {
            return u64::from(k == 0) as f64;
        }
        if self.p == 1.0 {
            return u64::from(k == self.n) as f64;
        }
        let kf = k as f64;
        let nf = self.n as f64;
        let ln_choose = lgamma(nf + 1.0) - lgamma(kf + 1.0) - lgamma(nf - kf + 1.0);
        (ln_choose + kf * self.p.ln() + (nf - kf) * (1.0 - self.p).ln()).exp()
    }
    fn cdf(&self, k: u64) -> f64 {
        if k >= self.n {
            return 1.0;
        }
        if self.p == 0.0 {
            return 1.0;
        }
        if self.p == 1.0 {
            return 0.0;
        }
        // P(X ≤ k) = I_{1−p}(n − k, k + 1).
        beta_reg((self.n - k) as f64, k as f64 + 1.0, 1.0 - self.p)
    }
    fn mean(&self) -> f64 {
        self.n as f64 * self.p
    }
    fn variance(&self) -> f64 {
        self.n as f64 * self.p * (1.0 - self.p)
    }
}

/// Kullback–Leibler divergence of two Gaussians (in nats):
///
/// ```text
/// KL(N(m1, v1) ‖ N(m2, v2)) = ln(v2/v1)/2 + (v1 + (m1 − m2)²) / (2 v2) − 1/2
/// ```
///
/// The exact closed form — no approximation, but only meaningful when both
/// inputs are genuine Gaussians with positive variance.
///
/// # Errors
/// [`DomainError`] when either variance is non-finite or non-positive.
pub fn kl_gaussian(m1: f64, v1: f64, m2: f64, v2: f64) -> Result<f64, DomainError> {
    require(
        "kl_gaussian",
        v1.is_finite() && v2.is_finite() && v1 > 0.0 && v2 > 0.0,
        "both variances must be finite and positive",
    )?;
    let diff = m1 - m2;
    Ok(0.5 * (v2 / v1).ln() + (v1 + diff * diff) / (2.0 * v2) - 0.5)
}

/// Bisection inversion of `f` toward `target` on `[lo, hi]`, assuming `f` is
/// non-decreasing, to ~1e-12 relative.
fn bisect(mut f: impl FnMut(f64) -> f64, target: f64, lo: f64, hi: f64) -> f64 {
    let mut lo = lo;
    let mut hi = hi;
    for _ in 0..200 {
        let mid = f64::midpoint(lo, hi);
        if mid == lo || mid == hi {
            break;
        }
        if f(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < 1e-12 * hi.abs().max(1e-12) {
            break;
        }
    }
    f64::midpoint(lo, hi)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // closed-form values asserted exactly
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn uniform_is_exact() {
        let u = Uniform::new(2.0, 6.0).unwrap();
        assert_eq!(u.mean(), 4.0);
        assert!((u.variance() - 16.0 / 12.0).abs() < 1e-15);
        assert_eq!(u.pdf(3.0), 0.25);
        assert_eq!(u.pdf(7.0), 0.0);
        assert_eq!(u.cdf(3.0), 0.25);
        assert_eq!(u.cdf(1.0), 0.0);
        assert_eq!(u.quantile(0.25), 3.0);
        assert_eq!(u.prob_greater_than(4.0), 0.5);
    }

    #[test]
    fn uniform_rejects_bad_bounds() {
        assert!(Uniform::new(5.0, 5.0).is_err());
        assert!(Uniform::new(f64::NAN, 1.0).is_err());
    }

    #[test]
    fn beta_known_values() {
        let b = Beta::new(2.0, 3.0).unwrap();
        assert_eq!(b.mean(), 0.4);
        assert!((b.variance() - 0.4 * 0.6 / 6.0).abs() < 1e-15);
        // pdf(0.5) = x^(a-1)(1-x)^(b-1)/B(a,b) with B(2,3) = 1/12
        // → 12·0.5·0.25 = 1.5.
        assert!(close(b.pdf(0.5), 12.0 * 0.5 * 0.25, 1e-12));
        // CDF of Beta(2,3): 6x^5 − 15x^4 + 10x^3 at 0.5 = 0.6875.
        assert!(close(b.cdf(0.5), 0.6875, 1e-12));
        // Quantile inverts the CDF.
        assert!(close(b.quantile(0.6875), 0.5, 1e-9));
        // Survival matches 1 − cdf away from the far tail.
        assert!(close(b.prob_greater_than(0.5), 1.0 - 0.6875, 1e-12));
    }

    #[test]
    fn beta_bernoulli_conjugate_update() {
        let prior = Beta::new(2.0, 2.0).unwrap();
        let posterior = prior.update_bernoulli(7, 3);
        assert_eq!(posterior.a(), 9.0);
        assert_eq!(posterior.b(), 5.0);
        // Mean shifts toward the observed 70% rate.
        assert!((posterior.mean() - 9.0 / 14.0).abs() < 1e-15);
        assert!(posterior.variance() < prior.variance(), "evidence tightens");
    }

    #[test]
    fn gamma_known_values() {
        let g = Gamma::new(2.0, 4.0).unwrap();
        assert_eq!(g.mean(), 0.5);
        assert_eq!(g.variance(), 0.125);
        // pdf(0.5) = r^k x^{k−1} e^{−rx} / Γ(k) = 16 · 0.5 · e^{−2}.
        assert!(close(g.pdf(0.5), 8.0 * (-2.0_f64).exp(), 1e-12));
        // Erlang CDF: P(2, x·r) = 1 − e^{−rx}(1 + rx).
        let x = 0.75;
        assert!(close(
            g.cdf(x),
            1.0 - (-4.0 * x).exp() * (1.0 + 4.0 * x),
            1e-12
        ));
        // Quantile round trip.
        let q = g.quantile(0.9);
        assert!(close(g.cdf(q), 0.9, 1e-9));
        // Upper tail is the direct Q form.
        assert!(close(g.prob_greater_than(q), 0.1, 1e-9));
    }

    #[test]
    fn gamma_poisson_conjugate_update() {
        let prior = Gamma::new(1.0, 1.0).unwrap();
        let posterior = prior.update_poisson(14, 5);
        assert_eq!(posterior.shape(), 15.0);
        assert_eq!(posterior.rate(), 6.0);
        assert!((posterior.mean() - 2.5).abs() < 1e-15);
        assert!(posterior.variance() < prior.variance());
    }

    #[test]
    fn lognormal_matches_the_normal_it_transforms() {
        let (m, v) = (0.5, 0.25);
        let ln = LogNormal::new(m, v).unwrap();
        // Mean e^{m+v/2}, variance (e^v − 1)e^{2m+v}.
        assert!(close(ln.mean(), (m + v / 2.0).exp(), 1e-12));
        assert!(close(
            ln.variance(),
            v.exp_m1() * (2.0 * m + v).exp(),
            1e-12
        ));
        // Median is e^m.
        assert!(close(ln.median(), 0.5_f64.exp(), 1e-12));
        // CDF/quantile round trip through the normal machinery.
        let q = ln.quantile(0.95);
        assert!(close(ln.cdf(q), 0.95, 1e-12));
        // The 50% quantile is the median.
        assert!(close(ln.quantile(0.5), ln.median(), 1e-12));
    }

    #[test]
    fn student_t_special_cases() {
        let t = StudentT::new(1.0).unwrap(); // Cauchy
        assert!(close(t.cdf(1.0), 0.5 + 1.0_f64.atan() / PI, 1e-12));
        assert!(close(t.cdf(-1.0), 0.5 - 1.0_f64.atan() / PI, 1e-12));
        assert!(t.variance().is_infinite());
        let t5 = StudentT::new(5.0).unwrap();
        assert!((t5.variance() - 5.0 / 3.0).abs() < 1e-15);
        // ν = 2: cdf(1) = 0.5 + 1/(2·√2)·(1/√(1+1/2))… use round trip instead.
        let q = t5.quantile(0.975);
        assert!(close(t5.cdf(q), 0.975, 1e-9));
        // Symmetry.
        assert!(close(t5.cdf(-q), 0.025, 1e-9));
        // Far upper tail keeps relative accuracy.
        let surv = t5.prob_greater_than(30.0);
        assert!(surv > 0.0 && surv < 1e-6, "{surv}");
    }

    #[test]
    fn poisson_matches_direct_sums() {
        let p = Poisson::new(2.7).unwrap();
        // pmf sums to ~1 over a wide range.
        let total: f64 = (0..40).map(|k| p.pmf(k)).sum();
        assert!(close(total, 1.0, 1e-12), "{total}");
        // CDF equals the direct pmf sum for small k.
        let direct: f64 = (0..=3).map(|k| p.pmf(k)).sum();
        assert!(close(p.cdf(3), direct, 1e-12));
        assert_eq!(p.mean(), 2.7);
        assert_eq!(p.variance(), 2.7);
        let degenerate = Poisson::new(0.0).unwrap();
        assert_eq!(degenerate.pmf(0), 1.0);
        assert_eq!(degenerate.cdf(10), 1.0);
    }

    #[test]
    fn binomial_matches_direct_sums() {
        let b = Binomial::new(20, 0.3).unwrap();
        let total: f64 = (0..=20).map(|k| b.pmf(k)).sum();
        assert!(close(total, 1.0, 1e-12), "{total}");
        let direct: f64 = (0..=5).map(|k| b.pmf(k)).sum();
        assert!(close(b.cdf(5), direct, 1e-12));
        assert!((b.mean() - 6.0).abs() < 1e-12);
        assert!((b.variance() - 4.2).abs() < 1e-12);
        // Degenerate endpoints.
        let sure = Binomial::new(4, 1.0).unwrap();
        assert_eq!(sure.pmf(4), 1.0);
        assert_eq!(sure.cdf(3), 0.0);
        assert_eq!(sure.cdf(4), 1.0);
    }

    #[test]
    fn constructors_reject_out_of_domain() {
        assert!(Beta::new(0.0, 1.0).is_err());
        assert!(Gamma::new(-1.0, 1.0).is_err());
        assert!(LogNormal::new(0.0, -1.0).is_err());
        assert!(StudentT::new(0.0).is_err());
        assert!(Poisson::new(-0.1).is_err());
        assert!(Binomial::new(5, 1.5).is_err());
        let err = Beta::new(-1.0, 1.0).unwrap_err();
        assert_eq!(err.family, "Beta");
    }
}
