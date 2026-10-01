//! Typed key-generation options, replacing the raw `&[u8]` parameter blob.
//!
//! [`ZkBackend::generate_keys`](crate::ZkBackend::generate_keys) takes backend
//! parameters as bytes for historical reasons (the trait shipped before any
//! backend had agreed on a parameter set). [`KeygenOptions`] is the typed form:
//! the two knobs every shipping adapter understands — the range-check width
//! and the halo2-style row bound `k` — expressed as validated fields, so a
//! caller cannot pass a nonsensical width and discover it inside a backend.
//!
//! The byte encoding is unchanged, so options encoded here and handed to an
//! adapter decode identically to the blob they replace.

use alloc::vec::Vec;

use crate::witness::DEFAULT_RANGE_BITS;

/// Typed key-generation parameters.
///
/// [`Default`] requests the backend defaults: the widest range-check chain
/// ([`DEFAULT_RANGE_BITS`]) and automatic sizing ([`KeygenOptions::k`] `None`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeygenOptions {
    /// Width in bits of the range-check chain used for named inputs and
    /// `NonNegative` constraints. Must be `1..=64`, which is every shipping
    /// backend's bit-decomposition width; anything wider is rejected rather
    /// than silently truncated.
    pub range_bits: u32,
    /// Log2 of the circuit's row bound, or `None` to let the backend size the
    /// circuit automatically.
    pub k: Option<u32>,
}

impl Default for KeygenOptions {
    fn default() -> Self {
        Self::defaults()
    }
}

impl KeygenOptions {
    /// The widest range-check chain any shipping backend can prove.
    pub const MAX_RANGE_BITS: u32 = DEFAULT_RANGE_BITS;

    /// The backend defaults.
    #[must_use]
    pub const fn defaults() -> Self {
        Self {
            range_bits: DEFAULT_RANGE_BITS,
            k: None,
        }
    }

    /// Validates and builds options.
    ///
    /// # Errors
    ///
    /// * [`KeygenOptionsError::RangeBitsTooWide`] — `range_bits` is 0 or above
    ///   [`KeygenOptions::MAX_RANGE_BITS`]; a zero-width chain cannot prove any
    ///   value and a chain wider than the backend's word cannot be proved at
    ///   all, so both are refused instead of being clamped.
    /// * [`KeygenOptionsError::RowBoundOutOfRange`] — `k` exceeds the backend
    ///   parameter byte (`0..=255`), so it could not be encoded.
    pub const fn new(range_bits: u32, k: Option<u32>) -> Result<Self, KeygenOptionsError> {
        if range_bits == 0 || range_bits > Self::MAX_RANGE_BITS {
            return Err(KeygenOptionsError::RangeBitsTooWide {
                bits: range_bits,
                max: Self::MAX_RANGE_BITS,
            });
        }
        if let Some(k) = k {
            if k > u8::MAX as u32 {
                return Err(KeygenOptionsError::RowBoundOutOfRange { k });
            }
        }
        Ok(Self { range_bits, k })
    }

    /// Options with only a range-check width, keeping automatic sizing.
    ///
    /// # Errors
    /// As [`KeygenOptions::new`].
    pub const fn with_range_bits(range_bits: u32) -> Result<Self, KeygenOptionsError> {
        Self::new(range_bits, None)
    }

    /// Returns options with the row bound set, re-validating the result.
    ///
    /// # Errors
    /// As [`KeygenOptions::new`].
    pub const fn with_k(self, k: u32) -> Result<Self, KeygenOptionsError> {
        match Self::new(self.range_bits, Some(k)) {
            Ok(valid) => Ok(valid),
            Err(_) => Err(KeygenOptionsError::RowBoundOutOfRange { k }),
        }
    }

    /// The row bound as the backend convention's `u8` (`0` = automatic).
    #[must_use]
    pub const fn k_or_auto(&self) -> u32 {
        match self.k {
            Some(k) => k,
            None => 0,
        }
    }

    /// Encodes to the byte form `ZkBackend::generate_keys` consumes.
    ///
    /// A byte is emitted only when it differs from the adapter's default, so
    /// [`KeygenOptions::defaults`] encodes to an empty slice and decodes back
    /// to the same options.
    #[must_use]
    pub fn encode(self) -> Vec<u8> {
        let mut out = Vec::new();
        if self.range_bits != DEFAULT_RANGE_BITS {
            out.push(u8::try_from(self.range_bits).unwrap_or(u8::MAX).max(1));
        }
        let k = self.k_or_auto();
        if k != 0 {
            out.push(u8::try_from(k).unwrap_or(u8::MAX));
        }
        out
    }

    /// Decodes the byte form.
    ///
    /// A truncated slice yields defaults and a first byte of `0` is read as
    /// "default width", matching the historical blob encoding. Out-of-range
    /// values are clamped rather than rejected, because decoding a blob has no
    /// error channel; build options with [`KeygenOptions::new`] when you want
    /// the width validated up front.
    #[must_use]
    pub fn decode(params: &[u8]) -> Self {
        let mut options = Self::defaults();
        if let Some(&bits) = params.first() {
            if bits != 0 {
                options.range_bits = u32::from(bits).min(Self::MAX_RANGE_BITS);
            }
        }
        options.k = match params.get(1) {
            None | Some(&0) => None,
            Some(&k) => Some(u32::from(k)),
        };
        options
    }
}
/// Why a set of [`KeygenOptions`] could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeygenOptionsError {
    /// The requested range-check width is zero (proves nothing) or wider than
    /// the backend's word (unprovable).
    RangeBitsTooWide {
        /// The rejected width.
        bits: u32,
        /// The widest width any backend accepts.
        max: u32,
    },
    /// The row bound does not fit the backend's parameter byte.
    RowBoundOutOfRange {
        /// The rejected `k`.
        k: u32,
    },
}

impl core::fmt::Display for KeygenOptionsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::RangeBitsTooWide { bits, max } => write!(
                f,
                "range_bits {bits} is not provable; use 1..={max} (a zero-width chain proves \
                 no values and a chain wider than the backend's word cannot be proved)"
            ),
            Self::RowBoundOutOfRange { k } => write!(
                f,
                "row bound k = {k} does not fit the backend parameter (0..=255)"
            ),
        }
    }
}

impl core::error::Error for KeygenOptionsError {}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn defaults_round_trip_through_the_legacy_blob() {
        let encoded = KeygenOptions::defaults().encode();
        assert!(encoded.is_empty());
        assert_eq!(KeygenOptions::decode(&encoded), KeygenOptions::defaults());
    }

    #[test]
    fn typed_options_encode_like_the_legacy_blob() {
        let options = KeygenOptions::with_range_bits(32)
            .expect("32 is a valid chain width")
            .with_k(12)
            .expect("12 fits the parameter byte");
        assert_eq!(options.encode(), vec![32, 12]);
        assert_eq!(KeygenOptions::decode(&options.encode()), options);
    }

    #[test]
    fn unencodable_widths_are_rejected_not_clamped() {
        assert_eq!(
            KeygenOptions::with_range_bits(0),
            Err(KeygenOptionsError::RangeBitsTooWide { bits: 0, max: 64 })
        );
        assert_eq!(
            KeygenOptions::with_range_bits(128),
            Err(KeygenOptionsError::RangeBitsTooWide { bits: 128, max: 64 })
        );
        assert!(KeygenOptions::with_range_bits(64).is_ok());
        assert!(KeygenOptions::with_range_bits(1).is_ok());
    }

    #[test]
    fn unencodable_row_bounds_are_rejected() {
        let base = KeygenOptions::defaults();
        assert_eq!(
            base.with_k(256),
            Err(KeygenOptionsError::RowBoundOutOfRange { k: 256 })
        );
        // A rejected row bound leaves the caller's options untouched.
        assert_eq!(base, KeygenOptions::defaults());
        assert_eq!(base.k_or_auto(), 0);
    }

    #[test]
    fn decoding_is_total_and_clamping() {
        // A first byte of 0 means "default width", as the historical blob
        // encoding specified.
        assert_eq!(
            KeygenOptions::decode(&[0, 9]),
            KeygenOptions {
                range_bits: 64,
                k: Some(9)
            }
        );
        // An absurd width from an untrusted blob clamps instead of panicking.
        assert_eq!(
            KeygenOptions::decode(&[255]),
            KeygenOptions {
                range_bits: 64,
                k: None
            }
        );
        // A trailing 0 means automatic sizing.
        assert_eq!(KeygenOptions::decode(&[32, 0]).k, None);
    }
}
