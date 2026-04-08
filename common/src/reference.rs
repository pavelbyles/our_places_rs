use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SupportedCountry {
    pub iso2char: &'static str,
    pub name: &'static str,
    pub tax_rate: Decimal,
}

impl SupportedCountry {
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
}
