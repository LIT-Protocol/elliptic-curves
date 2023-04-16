//! Field arithmetic modulo p = 0xa9fb57dba1eea9bc3e660a909d838d726e3bf623d52620282013481d1f6e5377
//!
//! Arithmetic implementations have been synthesized using fiat-crypto.
//!
//! # License
//!
//! Copyright (c) 2015-2020 the fiat-crypto authors
//!
//! fiat-crypto is distributed under the terms of the MIT License, the
//! Apache License (Version 2.0), and the BSD 1-Clause License;
//! users may pick which license to apply.

#[cfg_attr(target_pointer_width = "32", path = "field/bp256_32.rs")]
#[cfg_attr(target_pointer_width = "64", path = "field/bp256_64.rs")]
mod field_impl;

use self::field_impl::*;
use crate::{FieldBytes, U256};
use core::{
    fmt::{self, Debug},
    iter::{Product, Sum},
    ops::{AddAssign, MulAssign, Neg, SubAssign},
};
use elliptic_curve::{
    bigint::{ArrayEncoding, Integer, Limb},
    ff::PrimeField,
    subtle::{Choice, ConstantTimeEq, ConstantTimeLess, CtOption},
    Error, Result,
};

/// Constant representing the modulus serialized as hex.
const MODULUS_HEX: &str = "a9fb57dba1eea9bc3e660a909d838d726e3bf623d52620282013481d1f6e5377";

const MODULUS: U256 = U256::from_be_hex(MODULUS_HEX);

/// Element of the brainpoolP256's base field used for curve point coordinates.
#[derive(Clone, Copy)]
pub struct FieldElement(pub(super) U256);

impl FieldElement {
    /// Zero element.
    pub const ZERO: Self = Self(U256::ZERO);

    /// Multiplicative identity.
    pub const ONE: Self = Self::from_uint_unchecked(U256::ONE);

    /// Create a [`FieldElement`] from a canonical big-endian representation.
    pub fn from_bytes(field_bytes: &FieldBytes) -> CtOption<Self> {
        Self::from_uint(U256::from_be_byte_array(*field_bytes))
    }

    /// Decode [`FieldElement`] from a big endian byte slice.
    pub fn from_slice(slice: &[u8]) -> Result<Self> {
        if slice.len() == 32 {
            Option::from(Self::from_bytes(FieldBytes::from_slice(slice))).ok_or(Error)
        } else {
            Err(Error)
        }
    }

    /// Decode [`FieldElement`] from [`U256`] converting it into Montgomery form:
    ///
    /// ```text
    /// w * R^2 * R^-1 mod p = wR mod p
    /// ```
    pub fn from_uint(uint: U256) -> CtOption<Self> {
        let is_some = uint.ct_lt(&MODULUS);
        CtOption::new(Self::from_uint_unchecked(uint), is_some)
    }

    /// Parse a [`FieldElement`] from big endian hex-encoded bytes.
    ///
    /// Does *not* perform a check that the field element does not overflow the order.
    ///
    /// This method is primarily intended for defining internal constants.
    #[allow(dead_code)]
    pub(crate) const fn from_hex(hex: &str) -> Self {
        Self::from_uint_unchecked(U256::from_be_hex(hex))
    }

    /// Convert a `u64` into a [`FieldElement`].
    pub const fn from_u64(w: u64) -> Self {
        Self::from_uint_unchecked(U256::from_u64(w))
    }

    /// Decode [`FieldElement`] from [`U256`] converting it into Montgomery form.
    ///
    /// Does *not* perform a check that the field element does not overflow the order.
    ///
    /// Used incorrectly this can lead to invalid results!
    pub(crate) const fn from_uint_unchecked(w: U256) -> Self {
        Self(U256::from_words(fiat_bp256_to_montgomery(w.as_words())))
    }

    /// Returns the big-endian encoding of this [`FieldElement`].
    pub fn to_bytes(self) -> FieldBytes {
        self.0.to_be_byte_array()
    }

    /// Translate [`FieldElement`] out of the Montgomery domain, returning a
    /// [`U256`] in canonical form.
    #[inline]
    pub const fn to_canonical(self) -> U256 {
        U256::from_words(fiat_bp256_from_montgomery(self.0.as_words()))
    }

    /// Determine if this [`FieldElement`] is odd in the SEC1 sense: `self mod 2 == 1`.
    ///
    /// # Returns
    ///
    /// If odd, return `Choice(1)`.  Otherwise, return `Choice(0)`.
    pub fn is_odd(&self) -> Choice {
        self.to_canonical().is_odd()
    }

    /// Determine if this [`FieldElement`] is even in the SEC1 sense: `self mod 2 == 0`.
    ///
    /// # Returns
    ///
    /// If even, return `Choice(1)`.  Otherwise, return `Choice(0)`.
    pub fn is_even(&self) -> Choice {
        !self.is_odd()
    }

    /// Determine if this [`FieldElement`] is zero.
    ///
    /// # Returns
    ///
    /// If zero, return `Choice(1)`.  Otherwise, return `Choice(0)`.
    pub fn is_zero(&self) -> Choice {
        self.ct_eq(&Self::ZERO)
    }

    /// Add elements.
    pub const fn add(&self, rhs: &Self) -> Self {
        Self(U256::from_words(fiat_bp256_add(
            self.0.as_words(),
            rhs.0.as_words(),
        )))
    }

    /// Double element (add it to itself).
    #[must_use]
    pub const fn double(&self) -> Self {
        self.add(self)
    }

    /// Subtract elements.
    pub const fn sub(&self, rhs: &Self) -> Self {
        Self(U256::from_words(fiat_bp256_sub(
            self.0.as_words(),
            rhs.0.as_words(),
        )))
    }

    /// Multiply elements.
    pub const fn multiply(&self, rhs: &Self) -> Self {
        Self(U256::from_words(fiat_bp256_mul(
            self.0.as_words(),
            rhs.0.as_words(),
        )))
    }

    /// Negate element.
    pub const fn neg(&self) -> Self {
        Self(U256::from_words(fiat_bp256_opp(self.0.as_words())))
    }

    /// Compute modular square.
    #[must_use]
    pub const fn square(&self) -> Self {
        Self(U256::from_words(fiat_bp256_square(self.0.as_words())))
    }

    /// Returns `self^exp`, where `exp` is a little-endian integer exponent.
    ///
    /// **This operation is variable time with respect to the exponent.**
    ///
    /// If the exponent is fixed, this operation is effectively constant time.
    pub const fn pow_vartime(&self, exp: &[u64]) -> Self {
        let mut res = Self::ONE;
        let mut i = exp.len();

        while i > 0 {
            i -= 1;

            let mut j = 64;
            while j > 0 {
                j -= 1;
                res = res.square();

                if ((exp[i] >> j) & 1) == 1 {
                    res = res.multiply(self);
                }
            }
        }

        res
    }

    /// Returns the square root of self mod p, or `None` if no square root
    /// exists.
    pub fn sqrt(&self) -> CtOption<Self> {
        todo!("`sqrt` not implemented")
    }

    /// Compute [`FieldElement`] inversion: `1 / self`.
    pub fn invert(&self) -> CtOption<Self> {
        CtOption::new(self.invert_unchecked(), !self.is_zero())
    }

    /// Returns the multiplicative inverse of self.
    ///
    /// Does not check that self is non-zero.
    const fn invert_unchecked(&self) -> Self {
        let words = primeorder::impl_bernstein_yang_invert!(
            self.0.as_words(),
            Self::ONE.0.to_words(),
            256,
            U256::LIMBS,
            Limb,
            fiat_bp256_from_montgomery,
            fiat_bp256_mul,
            fiat_bp256_opp,
            fiat_bp256_divstep_precomp,
            fiat_bp256_divstep,
            fiat_bp256_msat,
            fiat_bp256_selectznz,
        );

        Self(U256::from_words(words))
    }
}

primeorder::impl_mont_field_element_arithmetic!(
    FieldElement,
    FieldBytes,
    U256,
    fiat_bp256_montgomery_domain_field_element,
    fiat_bp256_add,
    fiat_bp256_sub,
    fiat_bp256_mul,
    fiat_bp256_opp
);

impl Debug for FieldElement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FieldElement(0x{:X})", &self.0)
    }
}

impl PrimeField for FieldElement {
    type Repr = FieldBytes;

    const MODULUS: &'static str = MODULUS_HEX;
    const NUM_BITS: u32 = 256;
    const CAPACITY: u32 = 255;
    const TWO_INV: Self = Self::from_u64(2).invert_unchecked();
    const MULTIPLICATIVE_GENERATOR: Self = Self::ZERO; // TODO
    const S: u32 = 0; // TODO
    const ROOT_OF_UNITY: Self = Self::ZERO; // TODO
    const ROOT_OF_UNITY_INV: Self = Self::ZERO; // TODO
    const DELTA: Self = Self::ZERO; // TODO

    #[inline]
    fn from_repr(bytes: FieldBytes) -> CtOption<Self> {
        Self::from_bytes(&bytes)
    }

    #[inline]
    fn to_repr(&self) -> FieldBytes {
        self.to_bytes()
    }

    #[inline]
    fn is_odd(&self) -> Choice {
        self.is_odd()
    }
}