use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use crate::error::DomainError;

/// ISO 4217 alphabetic code. Amounts always travel as integer minor units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Currency(pub [u8; 3]);

impl Currency {
    pub const EUR: Self = Self(*b"EUR");
    pub const USD: Self = Self(*b"USD");
    pub const GBP: Self = Self(*b"GBP");

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).unwrap_or("???")
    }
}

impl Display for Currency {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for Currency {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Currency::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl FromStr for Currency {
    type Err = DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_ascii_uppercase();
        if s.len() != 3 || !s.chars().all(|c| c.is_ascii_alphabetic()) {
            return Err(DomainError::InvalidCurrency(s));
        }
        let mut buf = [0u8; 3];
        buf.copy_from_slice(s.as_bytes());
        Ok(Self(buf))
    }
}

/// Integer minor units. Never use floating point for money.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AmountMinor(i64);

impl AmountMinor {
    pub fn new(value: i64) -> Result<Self, DomainError> {
        if value <= 0 {
            return Err(DomainError::InvalidAmount);
        }
        Ok(Self(value))
    }

    pub fn zero() -> Self {
        Self(0)
    }

    pub fn get(self) -> i64 {
        self.0
    }
}

impl Display for AmountMinor {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount_minor: AmountMinor,
    pub currency: Currency,
}

impl Money {
    pub fn new(amount_minor: i64, currency: Currency) -> Result<Self, DomainError> {
        Ok(Self {
            amount_minor: AmountMinor::new(amount_minor)?,
            currency,
        })
    }
}
