//! Currencies as stored in the `currencies` reference table.
//!
//! Every currency code in the schema references `currencies(code)`, so the
//! list of usable currencies is data, not code. [`CurrencyCode`] validates the
//! shape of a code at the trust boundary; whether it exists or is active is a
//! database question.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use utoipa::ToSchema;
use uuid::Uuid;

/// Longest currency display name, in characters.
pub const MAX_CURRENCY_NAME_CHARS: usize = 64;

/// Why a string is not a valid currency code.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CurrencyCodeError {
    /// The code is not exactly three uppercase ASCII letters.
    #[error("currency code must be exactly 3 uppercase letters A-Z, got {0:?}")]
    InvalidFormat(String),
}

/// An ISO 4217-shaped currency code: exactly three uppercase ASCII letters.
///
/// Parsing never normalises (no trimming or upper-casing), so what is stored is
/// exactly what was validated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "String", into = "String")]
#[schema(value_type = String, example = "USD")]
pub struct CurrencyCode(String);

impl CurrencyCode {
    /// The code as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for CurrencyCode {
    type Error = CurrencyCodeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let bytes = value.as_bytes();
        (bytes.len() == 3 && bytes.iter().all(u8::is_ascii_uppercase))
            .then(|| Self(value.to_string()))
            .ok_or_else(|| CurrencyCodeError::InvalidFormat(value.to_string()))
    }
}

impl TryFrom<String> for CurrencyCode {
    type Error = CurrencyCodeError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl FromStr for CurrencyCode {
    type Err = CurrencyCodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

impl From<CurrencyCode> for String {
    fn from(code: CurrencyCode) -> Self {
        code.0
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for CurrencyCode {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

/// Why a currency create/update request is invalid.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CurrencyRequestError {
    /// The display name is blank or longer than [`MAX_CURRENCY_NAME_CHARS`].
    #[error("currency name must be 1-{MAX_CURRENCY_NAME_CHARS} characters")]
    InvalidName,
    /// An update changed nothing (neither name nor active flag given).
    #[error("update must set a name or the active flag")]
    EmptyUpdate,
}

/// Trims a display name and checks its length.
fn validate_name(name: &str) -> Result<String, CurrencyRequestError> {
    let trimmed = name.trim();
    let chars = trimmed.chars().count();
    (1..=MAX_CURRENCY_NAME_CHARS)
        .contains(&chars)
        .then(|| trimmed.to_string())
        .ok_or(CurrencyRequestError::InvalidName)
}

/// One row of the `currencies` reference table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct Currency {
    /// The currency code (primary key, immutable).
    pub code: CurrencyCode,
    /// Display name, e.g. "Jamaican Dollar".
    pub name: String,
    /// Whether the currency can be chosen for new pairs, listings, users and checkouts.
    pub active: bool,
}

/// A list of currencies, sorted by code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CurrenciesResponse {
    /// The currencies.
    pub currencies: Vec<Currency>,
}

/// Admin request to add a currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CreateCurrencyRequest {
    /// The new code.
    pub code: CurrencyCode,
    /// Display name (trimmed, 1-64 characters).
    pub name: String,
}

impl CreateCurrencyRequest {
    /// The request with its name trimmed, or why it is invalid.
    pub fn validated(self) -> Result<Self, CurrencyRequestError> {
        validate_name(&self.name).map(|name| Self { name, ..self })
    }
}

/// Admin request to rename and/or (de)activate a currency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpdateCurrencyRequest {
    /// New display name, if changing.
    #[serde(default)]
    pub name: Option<String>,
    /// New active flag, if changing.
    #[serde(default)]
    pub active: Option<bool>,
}

impl UpdateCurrencyRequest {
    /// The request with its name trimmed, or why it is invalid.
    pub fn validated(self) -> Result<Self, CurrencyRequestError> {
        match (&self.name, self.active) {
            (None, None) => Err(CurrencyRequestError::EmptyUpdate),
            (Some(name), active) => validate_name(name).map(|name| Self {
                name: Some(name),
                active,
            }),
            (None, Some(_)) => Ok(self),
        }
    }
}

/// A listing referenced in a "currency in use" rejection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ListingRef {
    /// Listing id.
    pub id: Uuid,
    /// Listing name.
    pub title: String,
    /// Listing slug, if any.
    pub slug: Option<String>,
}

/// Body detail of a `409 CURRENCY_IN_USE` rejection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CurrencyInUse {
    /// Live listings that block the change.
    pub listings: Vec<ListingRef>,
}

#[cfg(test)]
mod currency_tests {
    use super::*;

    #[test]
    fn accepts_three_uppercase_letters() {
        let code = CurrencyCode::try_from("USD").expect("valid");
        assert_eq!(code.as_str(), "USD");
        assert_eq!(code.to_string(), "USD");
    }

    #[test]
    fn rejects_lowercase() {
        assert!(CurrencyCode::try_from("usd").is_err());
        assert!(CurrencyCode::try_from("Usd").is_err());
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(CurrencyCode::try_from("US").is_err());
        assert!(CurrencyCode::try_from("USDD").is_err());
        assert!(CurrencyCode::try_from("").is_err());
    }

    #[test]
    fn rejects_non_letters_and_whitespace() {
        assert!(CurrencyCode::try_from("U$D").is_err());
        assert!(CurrencyCode::try_from("U1D").is_err());
        assert!(CurrencyCode::try_from(" USD").is_err());
        assert!(CurrencyCode::try_from("ÜSD").is_err());
    }

    #[test]
    fn serde_round_trips_and_rejects_invalid() {
        let code: CurrencyCode = serde_json::from_str("\"JMD\"").expect("valid json code");
        assert_eq!(serde_json::to_string(&code).expect("serialise"), "\"JMD\"");
        assert!(serde_json::from_str::<CurrencyCode>("\"jmd\"").is_err());
        assert!(
            serde_json::from_str::<CreateCurrencyRequest>(r#"{"code":"zz1","name":"x"}"#).is_err()
        );
    }

    #[test]
    fn create_trims_and_bounds_name() {
        let ok = CreateCurrencyRequest {
            code: CurrencyCode::try_from("TTD").expect("valid"),
            name: "  Trinidad Dollar ".into(),
        }
        .validated()
        .expect("valid name");
        assert_eq!(ok.name, "Trinidad Dollar");

        let blank = CreateCurrencyRequest {
            code: CurrencyCode::try_from("TTD").expect("valid"),
            name: "   ".into(),
        };
        assert_eq!(blank.validated(), Err(CurrencyRequestError::InvalidName));

        let long = CreateCurrencyRequest {
            code: CurrencyCode::try_from("TTD").expect("valid"),
            name: "x".repeat(MAX_CURRENCY_NAME_CHARS + 1),
        };
        assert_eq!(long.validated(), Err(CurrencyRequestError::InvalidName));
    }

    #[test]
    fn create_accepts_name_at_max_length() {
        let max = CreateCurrencyRequest {
            code: CurrencyCode::try_from("TTD").expect("valid"),
            name: "é".repeat(MAX_CURRENCY_NAME_CHARS),
        };
        assert!(
            max.validated().is_ok(),
            "length counts characters, not bytes"
        );
    }

    #[test]
    fn empty_update_is_rejected() {
        let empty = UpdateCurrencyRequest {
            name: None,
            active: None,
        };
        assert_eq!(empty.validated(), Err(CurrencyRequestError::EmptyUpdate));
    }

    #[test]
    fn update_validates_name_and_allows_active_only() {
        let toggle = UpdateCurrencyRequest {
            name: None,
            active: Some(false),
        };
        assert!(toggle.validated().is_ok());
        let blank = UpdateCurrencyRequest {
            name: Some(" ".into()),
            active: None,
        };
        assert_eq!(blank.validated(), Err(CurrencyRequestError::InvalidName));
    }
}
