//! Canonical multivariate polynomial normal form over `i64` coefficients.
//!
//! This is the core tool the rest of the crate builds on: normalizing two
//! arithmetic expressions (built from `+`, `-`, `*`, unary `-`, constants and
//! named variables — exactly the fragment [`tpt_axiom_ir::circuit::Expr`]
//! supports) into this canonical form and comparing them with `==` gives
//! *exact* equivalence checking, including for the bilinear/quadratic terms
//! that a linear-arithmetic (`QF_LRA`) solver cannot express.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::fmt;
use core::ops::{Add, Mul, Neg, Sub};

/// A monomial: a sorted multiset of variable ids. `[]` is the constant
/// monomial (degree 0); `[v]` is `v`; `[v, v]` is `v^2`; `[v, w]` is `v*w`.
type Monomial = Vec<usize>;

/// A polynomial in canonical form: a map from monomial to its (non-zero)
/// coefficient. Two polynomials are equal iff their canonical forms are
/// equal, regardless of how each was built up.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Polynomial {
    terms: BTreeMap<Monomial, i64>,
}

impl Polynomial {
    /// The zero polynomial.
    #[must_use]
    pub fn zero() -> Self {
        Self::default()
    }

    /// A constant polynomial.
    #[must_use]
    pub fn constant(value: i64) -> Self {
        let mut terms = BTreeMap::new();
        if value != 0 {
            terms.insert(Monomial::new(), value);
        }
        Self { terms }
    }

    /// The polynomial `v_id` (coefficient 1, degree 1).
    #[must_use]
    pub fn variable(id: usize) -> Self {
        let mut terms = BTreeMap::new();
        terms.insert(alloc::vec![id], 1);
        Self { terms }
    }

    /// `true` if this polynomial is identically zero.
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    fn add_term(&mut self, mono: Monomial, coeff: i64) {
        if coeff == 0 {
            return;
        }
        match self.terms.get_mut(&mono) {
            Some(existing) => {
                *existing += coeff;
                if *existing == 0 {
                    self.terms.remove(&mono);
                }
            }
            None => {
                self.terms.insert(mono, coeff);
            }
        }
    }
}

impl Add for Polynomial {
    type Output = Self;
    fn add(mut self, rhs: Self) -> Self {
        for (mono, coeff) in rhs.terms {
            self.add_term(mono, coeff);
        }
        self
    }
}

impl Sub for Polynomial {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        self + (-rhs)
    }
}

impl Neg for Polynomial {
    type Output = Self;
    fn neg(mut self) -> Self {
        for coeff in self.terms.values_mut() {
            *coeff = -*coeff;
        }
        self
    }
}

impl Mul for Polynomial {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let mut out = Self::zero();
        for (lm, lc) in &self.terms {
            for (rm, rc) in &rhs.terms {
                let mut mono: Monomial = lm.iter().chain(rm.iter()).copied().collect();
                mono.sort_unstable();
                out.add_term(mono, lc * rc);
            }
        }
        out
    }
}

impl fmt::Display for Polynomial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.terms.is_empty() {
            return write!(f, "0");
        }
        let mut first = true;
        for (mono, coeff) in &self.terms {
            if first {
                write!(f, "{coeff}")?;
                first = false;
            } else if *coeff >= 0 {
                write!(f, " + {coeff}")?;
            } else {
                write!(f, " - {}", -coeff)?;
            }
            for v in mono {
                write!(f, "*v{v}")?;
            }
        }
        Ok(())
    }
}

/// Two polynomials that were expected to be identical but aren't: the
/// generic "these two symbolic arithmetic expressions disagree" result
/// used across the crate (circuit-comparison mismatches, formula mismatches).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mismatch {
    /// The polynomial actually found (e.g. what a constraint encodes, or
    /// what an implementation's formula computes).
    pub found: Polynomial,
    /// The polynomial it was expected to equal (e.g. what the claimed
    /// comparison requires, or a reference formula).
    pub expected: Polynomial,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "expected `{}` but found `{}`",
            self.expected, self.found
        )
    }
}

/// Checks that two polynomials are identical, returning a [`Mismatch`] with
/// both rendered forms if not.
///
/// # Errors
///
/// Returns [`Mismatch`] if `found != expected`.
pub fn expect_equal(found: Polynomial, expected: Polynomial) -> Result<(), Mismatch> {
    if found == expected {
        Ok(())
    } else {
        Err(Mismatch { found, expected })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::ToString;

    #[test]
    fn constants_fold() {
        assert_eq!(Polynomial::constant(0), Polynomial::zero());
        assert_eq!(
            Polynomial::constant(2) + Polynomial::constant(3),
            Polynomial::constant(5)
        );
    }

    #[test]
    fn commutative_and_associative_like_ordinary_arithmetic() {
        let a = Polynomial::variable(0);
        let b = Polynomial::variable(1);
        assert_eq!(a.clone() + b.clone(), b + a);
    }

    #[test]
    fn mul_expands_and_collects_like_terms() {
        // (a + b) * (a - b) == a^2 - b^2
        let a = Polynomial::variable(0);
        let b = Polynomial::variable(1);
        let lhs = (a.clone() + b.clone()) * (a.clone() - b.clone());
        let rhs = a.clone() * a - b.clone() * b;
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn different_expressions_same_polynomial_are_equal() {
        // 2*(a+1) - a  ==  a + 2, built two completely different ways.
        let a = Polynomial::variable(0);
        let lhs = (Polynomial::constant(2) * (a.clone() + Polynomial::constant(1))) - a.clone();
        let rhs = a + Polynomial::constant(2);
        assert_eq!(lhs, rhs);
    }

    #[test]
    fn display_renders_a_readable_form() {
        let a = Polynomial::variable(0);
        let p = a.clone() * a - Polynomial::constant(4);
        assert_eq!(p.to_string(), "-4 + 1*v0*v0");
    }
}
