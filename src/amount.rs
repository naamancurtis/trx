//! A wrapper around [`Decimal`] in order to perform mathematic calculates on decimals with a
//! higher precision, alongside gaining some security benefits.

use rust_decimal::Decimal;
use serde::de::Error;
use serde::{Deserialize, Serialize};

use std::ops::{Add, AddAssign, Sub, SubAssign};

/// The precision we want to carry any decimal based operations to.
const PRECISION: u32 = 4;

/// A wrapper around [`Decimal`] in order to perform mathematic calculates on decimals with a
/// higher precision, alongside gaining some security benefits.
///
/// This uses Bankers Rounding rules as a [`rust_decimal::RoundingStrategy`]
///
/// This is a wrapper type for the transaction amount, it serves two primary
/// purposes.
///
/// 1. To enforce the requirement of carrying 4dp through the application
/// 2. To ensure that the transaction amount is not printable (ie. ![`std::fmt::Debug`]). Given that logs
///    could be shipped to an external 3rd party for processing, it is likely we wouldn't want to
///    log specific transaction amounts. If we did log transaction amounts and this data was
///    compromised, it would allow a malicious actor to establish the highest value clients, and
///    selectively target those clients.
///
/// # Notes
///
/// - Under the guise of this exercise, the intended **construction** method for this type is through
/// the deserialization of CSVs. As such the constructor is private, and no construction methods
/// are offered in the public API. This would be re-visited if requirements change.
/// - All instances of [`Amount`] that are created via `deserialization` will automatically
/// be rounded to 4 decimal places _using the bankers rounding rule_
/// - This type should not implement [`Deref`] or [`DerefMut`] without careful
/// consideration, as doing so would potentially allow [`Debug`] & [`Display`] implementations
/// through the dereferencing through to the [`Decimal`] type.
///
/// ## Debug & Display not allowed
///
/// Do not remove the two doc-tests below, they assure that [`Amount`] does not implement
/// [`Debug`] or [`Display`]
///
/// ```compile_fail
/// use lib::amount::Amount;
///
/// let csv_row = csv::StringRecord::from(vec!["1.03235"]);
/// let amount: Amount = csv_row.deserialize(None).expect("failed to deserialize csv row");
///
/// println!("{:?}", amount);
/// ```
///
/// ```compile_fail
/// use lib::amount::Amount;
///
/// let csv_row = csv::StringRecord::from(vec!["1.03235"]);
/// let amount: Amount = csv_row.deserialize(None).expect("failed to deserialize csv row");
///
/// println!("{}", amount);
/// ```
/// [`Debug`]: std::fmt::Debug
/// [`Display`]: std::fmt::Display
/// [`Deref`]: std::ops::Deref
/// [`DerefMut`]: std::ops::DerefMut
#[derive(PartialEq, PartialOrd, Clone, Copy, Serialize)]
pub struct Amount(Decimal);

impl Default for Amount {
    fn default() -> Self {
        Self(Decimal::ZERO)
    }
}

impl Amount {
    pub fn round(self) -> Self {
        Self(self.0.round_dp(PRECISION))
    }
}

impl Add<Amount> for Amount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Amount(self.0 + rhs.0)
    }
}

impl AddAssign for Amount {
    fn add_assign(&mut self, rhs: Self) {
        self.0 = self.0 + rhs.0
    }
}

impl Sub<Amount> for Amount {
    type Output = Self;

    fn sub(self, rhs: Amount) -> Self::Output {
        Amount(self.0 - rhs.0)
    }
}

impl SubAssign for Amount {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 = self.0 - rhs.0
    }
}

impl TryFrom<f32> for Amount {
    type Error = rust_decimal::Error;

    fn try_from(value: f32) -> Result<Self, Self::Error> {
        Ok(Self(Decimal::try_from(value)?).round())
    }
}

impl TryInto<f32> for Amount {
    type Error = rust_decimal::Error;

    fn try_into(self) -> Result<f32, Self::Error> {
        self.0.try_into()
    }
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bits = String::deserialize(deserializer)?;
        let decimal = Decimal::from_str_exact(&bits)
            .map(|d| d.round_dp(PRECISION))
            .map_err(|e| Error::custom(&format!("{}", e)))?;

        if decimal.is_sign_negative() {
            Err(Error::custom(
                "expected a value greater than or equal to 0.0",
            ))
        } else {
            Ok(Amount(decimal))
        }
    }
}

#[cfg(test)]
mod tests {
    use color_eyre::Result;

    use std::ops::Deref;

    use super::*;

    impl Amount {
        pub fn new(num: f32) -> Result<Self> {
            Ok(Amount(Decimal::try_from(num)?))
        }
    }

    // These tests deref Amount for the assertions, as we do not want Amount to implement [`fmt::Debug`]
    // however Decimal does. This implementation should be restricted to `cfg(test)`.

    impl Deref for Amount {
        type Target = Decimal;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    #[test]
    fn correctly_deserializes_f32() -> Result<()> {
        let csv_row = csv::StringRecord::from(vec!["1.032"]);
        let amount: Amount = csv_row.deserialize(None)?;
        assert_eq!(*amount, *Amount(Decimal::new(1032, 3)));
        Ok(())
    }

    #[test]
    fn automatically_rounds_to_4_dp_when_deserializing() -> Result<()> {
        let csv_row = csv::StringRecord::from(vec!["1.03235"]);
        let amount: Amount = csv_row.deserialize(None)?;
        assert_eq!(*amount, *Amount(Decimal::new(10324, 4)));
        Ok(())
    }

    #[test]
    fn follows_bankers_rounding() -> Result<()> {
        let csv_row = csv::StringRecord::from(vec!["1.03225"]);
        let amount: Amount = csv_row.deserialize(None)?;
        assert_eq!(*amount, *Amount(Decimal::new(10322, 4)));
        Ok(())
    }

    #[test]
    fn fails_to_deserialize_an_f32_less_than_0() -> Result<()> {
        let csv_row = csv::StringRecord::from(vec!["-1.032"]);
        let amount = csv_row.deserialize::<Amount>(None);
        assert!(amount.is_err());
        Ok(())
    }

    #[test]
    fn correctly_carries_out_add_operations() -> Result<()> {
        let lhs = Amount(Decimal::new(10234, 4));
        let rhs = Amount(Decimal::new(30923, 4));
        let expected = Amount(Decimal::new(41157, 4));
        let result = lhs + rhs;
        let mut assign_result = lhs;
        assign_result += rhs;
        assert_eq!(
            *result, *expected,
            "when performing an add operation expected {} to equal {}",
            result.0, expected.0
        );
        assert_eq!(
            *assign_result, *expected,
            "when performing an add assign operation expected {} to equal {}",
            assign_result.0, expected.0
        );
        Ok(())
    }

    #[test]
    fn correctly_carries_out_subtraction_operations() -> Result<()> {
        let lhs = Amount(Decimal::new(30923, 4));
        let rhs = Amount(Decimal::new(10234, 4));
        let expected = Amount(Decimal::new(20689, 4));
        let result = lhs - rhs;
        let mut assign_result = lhs;
        assign_result -= rhs;
        assert_eq!(
            *result, *expected,
            "when performing an subtraction operation expected {} to equal {}",
            result.0, expected.0
        );
        assert_eq!(
            *assign_result, *expected,
            "when performing an subtraction assign operation expected {} to equal {}",
            assign_result.0, expected.0
        );
        Ok(())
    }

    #[test]
    fn its_safe_to_coerce_max_decimal_to_f32() -> Result<()> {
        let dec = Decimal::MAX;
        let amt = Amount(dec);
        let _float: f32 = amt.try_into()?;
        Ok(())
    }

    #[test]
    fn its_safe_to_coerce_min_decimal_to_f32() -> Result<()> {
        let dec = Decimal::MIN;
        let amt = Amount(dec);
        let _float: f32 = amt.try_into()?;
        Ok(())
    }
}
