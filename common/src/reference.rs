use chrono::{DateTime, NaiveDate, TimeDelta, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// UTC offset of every MVP listing (Jamaica, UTC−5, no daylight saving).
pub const LISTING_UTC_OFFSET_SECS: i32 = -5 * 3600;

/// The calendar date at the listings right now (Jamaica time).
pub fn listing_today(now: DateTime<Utc>) -> NaiveDate {
    (now + TimeDelta::seconds(i64::from(LISTING_UTC_OFFSET_SECS))).date_naive()
}

/// Standard statutory fallback country code (Jamaica: 15% GCT).
pub const DEFAULT_TAX_COUNTRY: &str = "JM";

/// A country the platform operates in, with its tax rate.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SupportedCountry {
    /// ISO 3166-1 alpha-2 code.
    pub iso2char: &'static str,
    /// Country name.
    pub name: &'static str,
    /// Statutory tax rate as a fraction.
    pub tax_rate: Decimal,
}

impl SupportedCountry {
    /// Every supported country.
    pub const LIST: &'static [SupportedCountry] = &[
        SupportedCountry {
            iso2char: "JM",
            name: "Jamaica",
            tax_rate: rust_decimal::Decimal::from_parts(15, 0, 0, false, 2),
        },
        SupportedCountry {
            iso2char: "US",
            name: "United States",
            tax_rate: rust_decimal::Decimal::from_parts(0, 0, 0, false, 0),
        },
        SupportedCountry {
            iso2char: "CA",
            name: "Canada",
            tax_rate: rust_decimal::Decimal::from_parts(0, 0, 0, false, 0),
        },
        SupportedCountry {
            iso2char: "GB",
            name: "United Kingdom",
            tax_rate: rust_decimal::Decimal::from_parts(20, 0, 0, false, 2),
        },
    ];

    /// Resolves statutory tax rate for a country code or name, falling back to JM (15%) with a warning.
    pub fn tax_rate_for(query: &str) -> Decimal {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Self::default_tax_rate();
        }

        Self::LIST
            .iter()
            .find(|c| c.iso2char.eq_ignore_ascii_case(trimmed) || c.name.eq_ignore_ascii_case(trimmed))
            .map(|c| c.tax_rate)
            .unwrap_or_else(|| {
                tracing::warn!(
                    "Unsupported or unrecognized country '{}', falling back to default tax rate for {}",
                    trimmed,
                    DEFAULT_TAX_COUNTRY
                );
                Self::default_tax_rate()
            })
    }

    /// Returns the default tax rate (Jamaica: 15%).
    pub fn default_tax_rate() -> Decimal {
        Self::LIST
            .iter()
            .find(|c| c.iso2char == DEFAULT_TAX_COUNTRY)
            .map(|c| c.tax_rate)
            .unwrap_or_else(|| Decimal::from_parts(15, 0, 0, false, 2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_today_follows_jamaica_midnight() {
        let before = "2026-03-10T04:59:59Z".parse::<DateTime<Utc>>().expect("ts");
        let at = "2026-03-10T05:00:00Z".parse::<DateTime<Utc>>().expect("ts");
        assert_eq!(
            listing_today(before),
            NaiveDate::from_ymd_opt(2026, 3, 9).expect("d")
        );
        assert_eq!(
            listing_today(at),
            NaiveDate::from_ymd_opt(2026, 3, 10).expect("d")
        );
    }

    #[test]
    fn test_supported_country_tax_rate_resolution() {
        let jm_rate = Decimal::from_parts(15, 0, 0, false, 2);
        let gb_rate = Decimal::from_parts(20, 0, 0, false, 2);

        // ISO-2 code resolution
        assert_eq!(SupportedCountry::tax_rate_for("JM"), jm_rate);
        assert_eq!(SupportedCountry::tax_rate_for("jm"), jm_rate);
        assert_eq!(SupportedCountry::tax_rate_for("GB"), gb_rate);

        // Name resolution
        assert_eq!(SupportedCountry::tax_rate_for("Jamaica"), jm_rate);
        assert_eq!(SupportedCountry::tax_rate_for("jamaica"), jm_rate);

        // Fallback for unknown / empty country
        assert_eq!(SupportedCountry::tax_rate_for("Atlantis"), jm_rate);
        assert_eq!(SupportedCountry::tax_rate_for(""), jm_rate);
        assert_eq!(SupportedCountry::tax_rate_for("   "), jm_rate);
    }
}
