//! How money is shown to people: in the portals, on tax documents and in
//! emails. Line items carry the currency's symbol and thousands separators
//! (`$11,240.00`); standalone amounts and totals also name the currency
//! (`USD $14,133.00`). CSV exports keep plain figures and do not use this.

use rust_decimal::Decimal;

/// The symbol shown before amounts in `currency`: `$` for JMD, USD and CAD,
/// `£` for GBP, none for anything else.
pub fn currency_symbol(currency: &str) -> Option<&'static str> {
    match currency.to_ascii_uppercase().as_str() {
        "JMD" | "USD" | "CAD" => Some("$"),
        "GBP" => Some("£"),
        _ => None,
    }
}

/// A compact prefix for a headline price that can't be mistaken for another
/// currency: `USD` → `"US$"`, `JMD` → `"J$"`, `CAD` → `"CA$"` (all three use
/// `$`, so a bare `$` would be ambiguous), `GBP` → `"£"`, `EUR` → `"€"`, and
/// any other code followed by a space (`"TTD "`).
pub fn price_prefix(currency: &str) -> String {
    match currency.to_ascii_uppercase().as_str() {
        "USD" => "US$".to_string(),
        "JMD" => "J$".to_string(),
        "CAD" => "CA$".to_string(),
        "GBP" => "£".to_string(),
        "EUR" => "€".to_string(),
        other => format!("{other} "),
    }
}

/// An amount to 2 decimal places with comma thousands separators:
/// `11240` → `"11,240.00"`, `-325.5` → `"-325.50"`. Rounds the way pricing
/// does (`round_dp`); `{:.2}` on a `Decimal` would truncate instead.
pub fn grouped(amount: Decimal) -> String {
    let fixed = format!("{:.2}", amount.round_dp(2));
    let (sign, unsigned) = fixed
        .strip_prefix('-')
        .map_or(("", fixed.as_str()), |rest| ("-", rest));
    let (whole, cents) = unsigned.split_once('.').unwrap_or((unsigned, "00"));
    let digits: Vec<char> = whole.chars().collect();
    let with_commas: String = digits
        .iter()
        .enumerate()
        .flat_map(|(i, d)| {
            let comma = (i > 0 && (digits.len() - i).is_multiple_of(3)).then_some(',');
            comma.into_iter().chain(std::iter::once(*d))
        })
        .collect();
    format!("{sign}{with_commas}.{cents}")
}

/// A whole-unit amount with thousands separators, for headline prices:
/// `1650.4` → `"1,650"`.
pub fn grouped_whole(amount: Decimal) -> String {
    let fixed = grouped(amount.round_dp(0));
    fixed
        .strip_suffix(".00")
        .map_or_else(|| fixed.clone(), str::to_string)
}

/// A line-item amount: the currency's symbol (if it has one) and the grouped
/// figure, e.g. `"$11,240.00"`, `"£512.91"` or `"2,994.22"` for EUR. A
/// negative amount puts the sign before the symbol: `"-$325.50"`.
pub fn item_amount(currency: &str, amount: Decimal) -> String {
    let symbol = currency_symbol(currency).unwrap_or("");
    let figure = grouped(amount.abs());
    let sign = if amount.is_sign_negative() && !amount.is_zero() {
        "-"
    } else {
        ""
    };
    format!("{sign}{symbol}{figure}")
}

/// A total or standalone amount: currency code, symbol and grouped figure,
/// e.g. `"USD $14,133.00"`, `"GBP £2,879.12"` or `"EUR 3,361.47"`.
pub fn total_amount(currency: &str, amount: Decimal) -> String {
    format!("{currency} {}", item_amount(currency, amount))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn price_prefix_is_compact_and_never_ambiguous() {
        assert_eq!(price_prefix("USD"), "US$");
        assert_eq!(price_prefix("usd"), "US$");
        assert_eq!(price_prefix("JMD"), "J$");
        assert_eq!(price_prefix("CAD"), "CA$");
        assert_eq!(price_prefix("GBP"), "£");
        assert_eq!(price_prefix("EUR"), "€");
        assert_eq!(price_prefix("TTD"), "TTD ");
    }

    #[test]
    fn groups_thousands_and_pads_cents() {
        assert_eq!(grouped(dec!(0)), "0.00");
        assert_eq!(grouped(dec!(999.999)), "1,000.00");
        assert_eq!(grouped(dec!(11240)), "11,240.00");
        assert_eq!(grouped(dec!(1234567.5)), "1,234,567.50");
        assert_eq!(grouped(dec!(-325.5)), "-325.50");
        assert_eq!(grouped(dec!(-1234.5)), "-1,234.50");
        assert_eq!(grouped_whole(dec!(1650.4)), "1,650");
        assert_eq!(grouped_whole(dec!(650)), "650");
    }

    #[test]
    fn items_carry_the_symbol_and_totals_the_code() {
        assert_eq!(item_amount("USD", dec!(11240)), "$11,240.00");
        assert_eq!(item_amount("GBP", dec!(512.91)), "£512.91");
        assert_eq!(item_amount("EUR", dec!(2994.22)), "2,994.22");
        assert_eq!(item_amount("USD", dec!(-325.5)), "-$325.50");
        assert_eq!(total_amount("USD", dec!(14133)), "USD $14,133.00");
        assert_eq!(total_amount("JMD", dec!(2219000)), "JMD $2,219,000.00");
        assert_eq!(total_amount("EUR", dec!(3361.47)), "EUR 3,361.47");
    }
}
