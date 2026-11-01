//! Read-only view of the stored exchange rates (parity A28), for admins and
//! hosts. The legacy page showed hardcoded rates; these come from
//! `currency_exchange_rates`.

use common::exchange_rate::ExchangeRate;
use rust_decimal::Decimal;
use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::{View, view},
};
use web_client::get_api_client;

use crate::support::{service_token, staff};

/// USD pairs always listed, even before a rate is stored.
const USD_TARGETS: [&str; 4] = ["JMD", "EUR", "GBP", "CAD"];

/// One table row: a pair and its rate, if stored.
struct RateRow {
    pair: String,
    rate: Option<Decimal>,
    effective: String,
}

/// Stored rates plus a "Not set" row for each missing USD pair.
fn rows(rates: Vec<ExchangeRate>) -> Vec<RateRow> {
    let missing: Vec<RateRow> = USD_TARGETS
        .iter()
        .filter(|t| {
            !rates
                .iter()
                .any(|r| r.base_currency == "USD" && r.target_currency == **t)
        })
        .map(|t| RateRow {
            pair: format!("USD / {t}"),
            rate: None,
            effective: String::new(),
        })
        .collect();
    let mut all: Vec<RateRow> = rates
        .into_iter()
        .map(|r| RateRow {
            pair: format!("{} / {}", r.base_currency, r.target_currency),
            rate: Some(r.rate),
            effective: r.effective_at.format("%Y-%m-%d %H:%M UTC").to_string(),
        })
        .chain(missing)
        .collect();
    all.sort_by(|a, b| a.pair.cmp(&b.pair));
    all
}

/// The exchange rate table.
#[page("/admin/exchange-rates")]
pub async fn exchange_rates_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let token = service_token(cx, &user)?;
    let loaded = get_api_client(cx).get_exchange_rates_as(&token).await;
    let error = loaded.as_ref().err().map(|e| {
        tracing::info!("exchange rates unavailable: {e}");
        "Exchange rates could not be loaded. Try again shortly.".to_string()
    });
    let table = rows(loaded.map(|r| r.rates).unwrap_or_default());
    Ok(view! {
        <div class="space-y-6">
            <div>
                <p class="text-xs font-medium tracking-wide text-muted-foreground uppercase">"Tri-Currency Financial Engine"</p>
                <h1 class="text-2xl font-semibold">"Exchange Rates"</h1>
                <p class="text-sm text-muted-foreground">"The newest stored rate for each pair; pricing converts with these."</p>
            </div>
            if let Some(message) = error {
                <p role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">(message)</p>
            }
            <div class="overflow-x-auto rounded-xl border border-border">
                <table id="exchange-rates-table" class="w-full text-sm">
                    <thead class="bg-card text-left text-xs text-muted-foreground uppercase">
                        <tr>
                            <th scope="col" class="p-3">"Currency pair"</th>
                            <th scope="col" class="p-3 text-right">"Rate"</th>
                            <th scope="col" class="p-3">"Effective"</th>
                        </tr>
                    </thead>
                    <tbody class="divide-y divide-border">
                        for row in table {
                            <tr>
                                <td class="p-3 font-medium">(row.pair)</td>
                                <td class="p-3 text-right font-mono tabular-nums">
                                    (row.rate.map(|r| format!("{r:.6}")).unwrap_or_else(|| "Not set".to_string()))
                                </td>
                                <td class="p-3 text-muted-foreground">(row.effective)</td>
                            </tr>
                        }
                    </tbody>
                </table>
            </div>
        </div>
    })
}

#[cfg(test)]
mod tests {
    use super::rows;
    use chrono::Utc;
    use common::exchange_rate::ExchangeRate;
    use rust_decimal::Decimal;

    #[test]
    fn lists_missing_usd_pairs_as_not_set() {
        let stored = vec![ExchangeRate {
            base_currency: "USD".into(),
            target_currency: "JMD".into(),
            rate: Decimal::new(160, 0),
            effective_at: Utc::now(),
        }];
        let table = rows(stored);
        let pairs: Vec<_> = table
            .iter()
            .map(|r| (r.pair.as_str(), r.rate.is_some()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("USD / CAD", false),
                ("USD / EUR", false),
                ("USD / GBP", false),
                ("USD / JMD", true)
            ]
        );
    }
}
