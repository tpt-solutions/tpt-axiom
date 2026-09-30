//! Conversions between [`tpt_augur_std::Dist`] and the Axiom uncertainty
//! types.
//!
//! `tpt-augur` writes variables as distributions and runs Bayesian
//! inference; Axiom propagates uncertainty through arithmetic. The bridge is
//! **moment matching**: every Augur distribution has closed-form mean and
//! variance, so [`to_fuzzy_approx`] is exact for `Normal` (the only family
//! Axiom's first-order propagation models precisely) and mean/variance-exact
//! for the rest — higher moments are deliberately dropped, which is why the
//! conversion carries the `_approx` name.
//!
//! Conversions validate Augur's parameters (a `Normal(-, -1)` is a compile
//! artifact until sampled) and fail loudly on out-of-domain values.

use tpt_augur_std::Dist;
use tpt_axiom_core::{Bernoulli, Distribution, Fuzzy, InvalidProbability, Uncertain};

/// A conversion failure: the Augur distribution's parameters were outside
/// the domain its math requires.
#[derive(Clone, Debug, PartialEq)]
pub enum ConversionError {
    /// A parameter violated the distribution's domain (e.g. `sigma <= 0`).
    InvalidParameter {
        /// Which distribution was rejected.
        dist: &'static str,
        /// The human-readable domain violation.
        reason: String,
    },
    /// An Axiom-side validation failed (e.g. non-finite moments).
    InvalidProbability(InvalidProbability),
}

impl core::fmt::Display for ConversionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidParameter { dist, reason } => {
                write!(f, "augur {dist}: {reason}")
            }
            Self::InvalidProbability(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ConversionError {}

impl From<InvalidProbability> for ConversionError {
    fn from(e: InvalidProbability) -> Self {
        Self::InvalidProbability(e)
    }
}

use std::string::String;

fn require(cond: bool, dist: &'static str, reason: &str) -> Result<(), ConversionError> {
    if cond {
        Ok(())
    } else {
        Err(ConversionError::InvalidParameter {
            dist,
            reason: String::from(reason),
        })
    }
}

/// Converts an Augur distribution into the Axiom distribution family.
///
/// Exact for `Normal` (Axiom's `Distribution::Gaussian` *is* that family);
/// `Bernoulli` rides the dedicated [`to_bernoulli`]. Other families have no
/// exact Axiom representation — use [`to_fuzzy_approx`] for their moments.
///
/// # Errors
/// [`ConversionError::InvalidParameter`] when the parameters are
/// out-of-domain.
pub fn to_distribution(dist: Dist) -> Result<Distribution<f64>, ConversionError> {
    match dist {
        Dist::Normal { mu, sigma } => {
            require(sigma.is_finite() && sigma >= 0.0, "Normal", "sigma must be >= 0")?;
            // A degenerate (zero-sigma) Normal *is* a point mass: map it to
            // the exact Axiom representation instead of erroring, so a
            // round trip through `from_distribution` is lossless.
            let variance = sigma * sigma;
            if variance == 0.0 {
                return Ok(Distribution::Constant(mu));
            }
            if !variance.is_finite() {
                return Err(ConversionError::InvalidParameter {
                    dist: "Normal",
                    reason: String::from("sigma^2 overflows to infinity"),
                });
            }
            Ok(Distribution::gaussian(mu, variance))
        }
        other => Ok(to_fuzzy_approx(other)?.into()),
    }
}

/// Moment-matches any Augur distribution into a [`Fuzzy<f64>`].
///
/// The name says *approx* on purpose: only `Normal` is
/// information-preserving. Every other family keeps its exact closed-form
/// mean and variance and **deliberately drops its higher moments** - a
/// Gamma and a Gaussian with matched moments are different distributions,
/// and after this conversion Axiom only ever sees the Gaussian-shaped
/// summary. Callers that need the distinction should keep the original
/// `Dist` and use this only at the arithmetic boundary.
///
/// # Errors
/// [`ConversionError::InvalidParameter`] when the parameters are
/// out-of-domain, [`ConversionError::InvalidProbability`] if the matched
/// moments are not finite.
pub fn to_fuzzy_approx(dist: Dist) -> Result<Fuzzy<f64>, ConversionError> {
    let (mean, variance) = match dist {
        Dist::Normal { mu, sigma } => {
            require(
                sigma.is_finite() && sigma > 0.0,
                "Normal",
                "sigma must be > 0",
            )?;
            (mu, sigma * sigma)
        }
        Dist::HalfNormal { sigma } => {
            require(
                sigma.is_finite() && sigma > 0.0,
                "HalfNormal",
                "sigma must be > 0",
            )?;
            let two_over_pi = 2.0 / core::f64::consts::PI;
            (
                sigma * (two_over_pi).sqrt(),
                sigma * sigma * (1.0 - two_over_pi),
            )
        }
        Dist::Beta { a, b } => {
            require(
                a.is_finite() && a > 0.0 && b.is_finite() && b > 0.0,
                "Beta",
                "a and b must be > 0",
            )?;
            let sum = a + b;
            // mean = a / (a+b); variance = ab / ((a+b)^2 (a+b+1)) factored
            // to keep clippy's grouping heuristic happy:
            // (a/sum) * (b/sum) / (sum+1).
            (a / sum, (a / sum) * (b / sum) / (sum + 1.0))
        }
        Dist::Gamma { shape, rate } => {
            require(
                shape.is_finite() && shape > 0.0 && rate.is_finite() && rate > 0.0,
                "Gamma",
                "shape and rate must be > 0",
            )?;
            (shape / rate, shape / (rate * rate))
        }
        Dist::Uniform { lo, hi } => {
            require(
                hi.is_finite() && lo.is_finite() && hi > lo,
                "Uniform",
                "hi must be > lo",
            )?;
            // `lo + (hi - lo) / 2` is midpoint without the overflow
            // `f64::midpoint` (Rust 1.86) would give us for free.
            (lo + (hi - lo) / 2.0, (hi - lo) * (hi - lo) / 12.0)
        }
        Dist::Exponential { rate } => {
            require(
                rate.is_finite() && rate > 0.0,
                "Exponential",
                "rate must be > 0",
            )?;
            (1.0 / rate, 1.0 / (rate * rate))
        }
        Dist::Binomial { n, p } => {
            require(n.is_finite() && n >= 0.0, "Binomial", "n must be >= 0")?;
            require(
                p.is_finite() && (0.0..=1.0).contains(&p),
                "Binomial",
                "p must lie in [0, 1]",
            )?;
            (n * p, n * p * (1.0 - p))
        }
        Dist::Poisson { rate } => {
            require(
                rate.is_finite() && rate >= 0.0,
                "Poisson",
                "rate must be >= 0",
            )?;
            (rate, rate)
        }
        Dist::Bernoulli { p } => {
            require(
                p.is_finite() && (0.0..=1.0).contains(&p),
                "Bernoulli",
                "p must lie in [0, 1]",
            )?;
            (p, p * (1.0 - p))
        }
    };
    if !mean.is_finite() || !variance.is_finite() {
        return Err(ConversionError::InvalidParameter {
            dist: "matched",
            reason: String::from("moments must be finite"),
        });
    }
    Ok(Fuzzy::new(mean, variance))
}

/// Converts an Augur distribution into an [`Uncertain`] estimate: the same
/// moment matching as [`to_fuzzy_approx`], wrapped with its mean as the
/// estimate.
///
/// # Errors
/// Same as [`to_fuzzy_approx`].
pub fn to_uncertain_approx(dist: Dist) -> Result<Uncertain<f64>, ConversionError> {
    let fuzzy = to_fuzzy_approx(dist)?;
    Ok(Uncertain::Estimated(fuzzy))
}

/// Extracts an Axiom [`Bernoulli`] from an Augur `Dist::Bernoulli`.
///
/// # Errors
/// [`ConversionError::InvalidParameter`] for any other family or an
/// out-of-range `p`.
pub fn to_bernoulli(dist: Dist) -> Result<Bernoulli, ConversionError> {
    match dist {
        Dist::Bernoulli { p } => Bernoulli::new(p).map_err(ConversionError::from),
        other => Err(ConversionError::InvalidParameter {
            dist: family_name(&other),
            reason: String::from("not a Bernoulli distribution"),
        }),
    }
}

/// Converts an Axiom distribution back into an Augur `Dist::Normal` (the
/// exact inverse of the `Normal` arm of [`to_distribution`]: a
/// zero-variance Gaussian/`Constant` becomes a zero-sigma `Normal`).
///
/// # Errors
/// [`ConversionError::InvalidParameter`] when the moments are not finite or
/// the variance is negative or NaN. (The previous `variance.max(0.0)`
/// silently turned a NaN variance into a zero-sigma normal - swallowing the
/// invalid state instead of reporting it.)
pub fn from_distribution(dist: &Distribution<f64>) -> Result<Dist, ConversionError> {
    let (mu, variance) = match *dist {
        Distribution::Gaussian { mean, variance } => (mean, variance),
        Distribution::Constant(value) => (value, 0.0),
    };
    if !mu.is_finite() || !variance.is_finite() || variance < 0.0 {
        return Err(ConversionError::InvalidParameter {
            dist: "Normal",
            reason: String::from("moments must be finite with a non-negative variance"),
        });
    }
    Ok(Dist::Normal {
        mu,
        sigma: variance.sqrt(),
    })
}

/// Converts a [`Fuzzy`] estimate into an Augur `Dist::Normal` with the same
/// mean and standard deviation.
///
/// # Errors
/// [`ConversionError::InvalidParameter`] when the moments are not finite or
/// the variance is negative or NaN.
pub fn from_fuzzy(fuzzy: Fuzzy<f64>) -> Result<Dist, ConversionError> {
    if !fuzzy.mean().is_finite() || !fuzzy.variance().is_finite() || fuzzy.variance() < 0.0 {
        return Err(ConversionError::InvalidParameter {
            dist: "Normal",
            reason: String::from("moments must be finite with a non-negative variance"),
        });
    }
    Ok(Dist::Normal {
        mu: fuzzy.mean(),
        sigma: fuzzy.variance().sqrt(),
    })
}

const fn family_name(dist: &Dist) -> &'static str {
    match dist {
        Dist::Normal { .. } => "Normal",
        Dist::HalfNormal { .. } => "HalfNormal",
        Dist::Beta { .. } => "Beta",
        Dist::Gamma { .. } => "Gamma",
        Dist::Uniform { .. } => "Uniform",
        Dist::Exponential { .. } => "Exponential",
        Dist::Binomial { .. } => "Binomial",
        Dist::Poisson { .. } => "Poisson",
        Dist::Bernoulli { .. } => "Bernoulli",
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // exact-value assertions on analytic results

    use super::*;

    #[test]
    fn normal_roundtrips_exactly() {
        let dist = Dist::Normal {
            mu: 10.5,
            sigma: 2.0,
        };
        let fuzzy = to_fuzzy_approx(dist).unwrap();
        assert_eq!(fuzzy.mean(), 10.5);
        assert_eq!(fuzzy.variance(), 4.0);
        match from_fuzzy(fuzzy).unwrap() {
            Dist::Normal { mu, sigma } => {
                assert_eq!(mu, 10.5);
                assert_eq!(sigma, 2.0);
            }
            other => panic!("expected Normal, got {other:?}"),
        }
        match to_distribution(dist).unwrap() {
            Distribution::Gaussian { mean, variance } => {
                assert_eq!(mean, 10.5);
                assert_eq!(variance, 4.0);
            }
            other @ Distribution::Constant(_) => panic!("expected Gaussian, got {other:?}"),
        }
    }

    #[test]
    fn zero_sigma_normal_roundtrips_through_constant() {
        // sigma == 0 is a point mass: accepted, mapped to Constant, and the
        // round trip back is lossless (previously an error).
        let point = Dist::Normal { mu: 7.5, sigma: 0.0 };
        let dist = to_distribution(point).unwrap();
        assert_eq!(dist, Distribution::Constant(7.5));
        match from_distribution(&dist).unwrap() {
            Dist::Normal { mu, sigma } => {
                assert_eq!(mu, 7.5);
                assert_eq!(sigma, 0.0);
            }
            other => panic!("expected Normal, got {other:?}"),
        }
    }

    #[test]
    fn huge_sigma_is_caught_after_squaring() {
        // sigma = 1e200 is finite, but sigma^2 overflows: the conversion
        // must reject it instead of storing an infinite variance.
        let err = to_distribution(Dist::Normal { mu: 0.0, sigma: 1e200 }).unwrap_err();
        assert!(err.to_string().contains("overflows"), "{err}");
    }

    #[test]
    fn from_distribution_rejects_nan_instead_of_swallowing() {
        // A NaN variance used to be flattened to a zero-sigma normal by
        // `variance.max(0.0)`; now it is reported.
        let bad = Distribution::Gaussian { mean: 0.0, variance: f64::NAN };
        assert!(from_distribution(&bad).is_err());
        assert!(from_fuzzy(Fuzzy::new(f64::NAN, 1.0)).is_err());
    }

    #[test]
    fn closed_form_moments_match_textbook_values() {
        // Uniform(0, 12): mean 6, variance 144/12 = 12.
        let u = to_fuzzy_approx(Dist::Uniform { lo: 0.0, hi: 12.0 }).unwrap();
        assert_eq!(u.mean(), 6.0);
        assert_eq!(u.variance(), 12.0);
        // Gamma(k=2, rate=4): mean 0.5, variance 2/16 = 0.125.
        let g = to_fuzzy_approx(Dist::Gamma {
            shape: 2.0,
            rate: 4.0,
        })
        .unwrap();
        assert_eq!(g.mean(), 0.5);
        assert_eq!(g.variance(), 0.125);
        // Beta(2, 2): mean 0.5, variance 4/(16*5) = 0.05.
        let b = to_fuzzy_approx(Dist::Beta { a: 2.0, b: 2.0 }).unwrap();
        assert_eq!(b.variance(), 0.05);
        // Poisson(9): mean = variance = 9.
        let p = to_fuzzy_approx(Dist::Poisson { rate: 9.0 }).unwrap();
        assert_eq!(p.mean(), 9.0);
        assert_eq!(p.variance(), 9.0);
        // Binomial(n=10, p=0.3): mean 3, variance 2.1.
        let bi = to_fuzzy_approx(Dist::Binomial { n: 10.0, p: 0.3 }).unwrap();
        assert!((bi.mean() - 3.0).abs() < 1e-12);
        assert!((bi.variance() - 2.1).abs() < 1e-12);
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        assert!(
            to_fuzzy_approx(Dist::Normal {
                mu: 0.0,
                sigma: -1.0
            })
            .is_err()
        );
        assert!(
            to_fuzzy_approx(Dist::Gamma {
                shape: 0.0,
                rate: 1.0
            })
            .is_err()
        );
        assert!(to_fuzzy_approx(Dist::Uniform { lo: 5.0, hi: 5.0 }).is_err());
        assert!(to_fuzzy_approx(Dist::Bernoulli { p: 1.5 }).is_err());
        // A *negative* sigma is still rejected; only exactly-zero maps to a
        // point mass.
        let err = to_distribution(Dist::Normal {
            mu: 0.0,
            sigma: -1.0,
        })
        .unwrap_err();
        assert!(err.to_string().contains("sigma"), "{err}");
    }

    #[test]
    fn bernoulli_conversion_is_exact() {
        let b = to_bernoulli(Dist::Bernoulli { p: 0.6 }).unwrap();
        assert_eq!(b.p().value(), 0.6);
        assert!(
            to_bernoulli(Dist::Normal {
                mu: 0.0,
                sigma: 1.0
            })
            .is_err()
        );
    }

    #[test]
    fn uncertain_wraps_the_estimate() {
        let u = to_uncertain_approx(Dist::Exponential { rate: 2.0 }).unwrap();
        match u {
            Uncertain::Estimated(estimate) => {
                assert_eq!(estimate.mean(), 0.5);
                assert_eq!(estimate.variance(), 0.25);
            }
            Uncertain::Certain(_) => panic!("expected an estimate"),
        }
    }

    #[test]
    fn axiom_gaussians_map_back_to_augur() {
        let dist = Distribution::<f64>::gaussian(3.0, 4.0);
        match from_distribution(&dist).unwrap() {
            Dist::Normal { mu, sigma } => {
                assert_eq!(mu, 3.0);
                assert_eq!(sigma, 2.0);
            }
            other => panic!("expected Normal, got {other:?}"),
        }
    }
}
