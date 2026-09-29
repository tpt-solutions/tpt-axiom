//! Conversions between [`tpt_augur_std::Dist`] and the Axiom uncertainty
//! types.
//!
//! `tpt-augur` writes variables as distributions and runs Bayesian
//! inference; Axiom propagates uncertainty through arithmetic. The bridge is
//! **moment matching**: every Augur distribution has closed-form mean and
//! variance, so [`to_fuzzy`] is exact for `Normal` (the only family Axiom's
//! first-order propagation models precisely) and mean/variance-exact for the
//! rest — higher moments are deliberately dropped, which is the right lossy
//! boundary for feeding sampled values into uncertainty arithmetic.
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
/// exact Axiom representation — use [`to_fuzzy`] for their moments.
///
/// # Errors
/// [`ConversionError::InvalidParameter`] when the parameters are
/// out-of-domain.
pub fn to_distribution(dist: Dist) -> Result<Distribution<f64>, ConversionError> {
    match dist {
        Dist::Normal { mu, sigma } => {
            require(
                sigma.is_finite() && sigma > 0.0,
                "Normal",
                "sigma must be > 0",
            )?;
            Ok(Distribution::gaussian(mu, sigma * sigma))
        }
        other => Ok(to_fuzzy(other)?.into()),
    }
}

/// Moment-matches any Augur distribution into a [`Fuzzy<f64>`].
///
/// The mean and variance are the distribution's exact closed-form moments;
/// only the higher moments are lost. `Normal` is information-preserving.
///
/// # Errors
/// [`ConversionError::InvalidParameter`] when the parameters are
/// out-of-domain, [`ConversionError::InvalidProbability`] if the matched
/// moments are not finite.
pub fn to_fuzzy(dist: Dist) -> Result<Fuzzy<f64>, ConversionError> {
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
/// moment matching as [`to_fuzzy`], wrapped with its mean as the estimate.
///
/// # Errors
/// Same as [`to_fuzzy`].
pub fn to_uncertain(dist: Dist) -> Result<Uncertain<f64>, ConversionError> {
    let fuzzy = to_fuzzy(dist)?;
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

/// Converts an Axiom Gaussian back into an Augur `Dist::Normal` (the exact
/// inverse of the `Normal` arm of [`to_distribution`]).
///
/// `Constant` distributions become zero-sigma normals; other families have
/// no faithful Augur encoding because Axiom's `Distribution` is
/// Gaussian-shaped by design.
#[must_use]
pub fn from_distribution(dist: &Distribution<f64>) -> Dist {
    match *dist {
        Distribution::Gaussian { mean, variance } => Dist::Normal {
            mu: mean,
            sigma: variance.max(0.0).sqrt(),
        },
        Distribution::Constant(value) => Dist::Normal {
            mu: value,
            sigma: 0.0,
        },
    }
}

/// Converts a [`Fuzzy`] estimate into an Augur `Dist::Normal` with the same
/// mean and standard deviation.
#[must_use]
pub fn from_fuzzy(fuzzy: Fuzzy<f64>) -> Dist {
    Dist::Normal {
        mu: fuzzy.mean(),
        sigma: fuzzy.variance().max(0.0).sqrt(),
    }
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
        let fuzzy = to_fuzzy(dist).unwrap();
        assert_eq!(fuzzy.mean(), 10.5);
        assert_eq!(fuzzy.variance(), 4.0);
        match from_fuzzy(fuzzy) {
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
    fn closed_form_moments_match_textbook_values() {
        // Uniform(0, 12): mean 6, variance 144/12 = 12.
        let u = to_fuzzy(Dist::Uniform { lo: 0.0, hi: 12.0 }).unwrap();
        assert_eq!(u.mean(), 6.0);
        assert_eq!(u.variance(), 12.0);
        // Gamma(k=2, rate=4): mean 0.5, variance 2/16 = 0.125.
        let g = to_fuzzy(Dist::Gamma {
            shape: 2.0,
            rate: 4.0,
        })
        .unwrap();
        assert_eq!(g.mean(), 0.5);
        assert_eq!(g.variance(), 0.125);
        // Beta(2, 2): mean 0.5, variance 4/(16*5) = 0.05.
        let b = to_fuzzy(Dist::Beta { a: 2.0, b: 2.0 }).unwrap();
        assert_eq!(b.variance(), 0.05);
        // Poisson(9): mean = variance = 9.
        let p = to_fuzzy(Dist::Poisson { rate: 9.0 }).unwrap();
        assert_eq!(p.mean(), 9.0);
        assert_eq!(p.variance(), 9.0);
        // Binomial(n=10, p=0.3): mean 3, variance 2.1.
        let bi = to_fuzzy(Dist::Binomial { n: 10.0, p: 0.3 }).unwrap();
        assert!((bi.mean() - 3.0).abs() < 1e-12);
        assert!((bi.variance() - 2.1).abs() < 1e-12);
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        assert!(
            to_fuzzy(Dist::Normal {
                mu: 0.0,
                sigma: -1.0
            })
            .is_err()
        );
        assert!(
            to_fuzzy(Dist::Gamma {
                shape: 0.0,
                rate: 1.0
            })
            .is_err()
        );
        assert!(to_fuzzy(Dist::Uniform { lo: 5.0, hi: 5.0 }).is_err());
        assert!(to_fuzzy(Dist::Bernoulli { p: 1.5 }).is_err());
        let err = to_distribution(Dist::Normal {
            mu: 0.0,
            sigma: 0.0,
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
        let u = to_uncertain(Dist::Exponential { rate: 2.0 }).unwrap();
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
        match from_distribution(&dist) {
            Dist::Normal { mu, sigma } => {
                assert_eq!(mu, 3.0);
                assert_eq!(sigma, 2.0);
            }
            other => panic!("expected Normal, got {other:?}"),
        }
    }
}
