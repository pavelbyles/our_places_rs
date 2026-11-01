//! Host payout ledger and summaries, and admin payout status.

use super::{booking_api_audience, booking_api_url, get_client};
use anyhow::{Context, Result, bail};
use uuid::Uuid;

pub async fn get_host_payout_ledger(
    filter: &crate::payout::PayoutFilter,
) -> Result<crate::payout::PayoutLedgerResponse> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let mut params = Vec::new();
    if let Some(listing_id) = filter.listing_id {
        params.push(format!("listing_id={}", listing_id));
    }
    if let Some(host_id) = filter.host_id {
        params.push(format!("host_id={}", host_id));
    }
    if let Some(status) = filter.status {
        params.push(format!("status={}", status));
    }
    if let Some(date_from) = filter.date_from {
        params.push(format!("date_from={}", date_from));
    }
    if let Some(date_to) = filter.date_to {
        params.push(format!("date_to={}", date_to));
    }
    if let Some(page) = filter.page {
        params.push(format!("page={}", page));
    }
    if let Some(per_page) = filter.per_page {
        params.push(format!("per_page={}", per_page));
    }
    let query_str = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    let url = format!("{}/api/v1/hosts/ledger{}", api_url, query_str);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch payout ledger ({}): {}", status, err_text);
    }

    res.json::<crate::payout::PayoutLedgerResponse>()
        .await
        .context("Failed to parse payout ledger response")
}

pub async fn get_host_payout_summary(
    filter: &crate::payout::PayoutFilter,
) -> Result<crate::payout::PayoutSummary> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let mut params = Vec::new();
    if let Some(listing_id) = filter.listing_id {
        params.push(format!("listing_id={}", listing_id));
    }
    if let Some(host_id) = filter.host_id {
        params.push(format!("host_id={}", host_id));
    }
    if let Some(status) = filter.status {
        params.push(format!("status={}", status));
    }
    if let Some(date_from) = filter.date_from {
        params.push(format!("date_from={}", date_from));
    }
    if let Some(date_to) = filter.date_to {
        params.push(format!("date_to={}", date_to));
    }
    let query_str = if params.is_empty() {
        String::new()
    } else {
        format!("?{}", params.join("&"))
    };
    let url = format!("{}/api/v1/hosts/ledger/summary{}", api_url, query_str);

    let res = get_client()
        .get(&url, &audience)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to fetch payout summary ({}): {}", status, err_text);
    }

    res.json::<crate::payout::PayoutSummary>()
        .await
        .context("Failed to parse payout summary response")
}

pub async fn update_admin_payout_status(
    id: Uuid,
    req: &crate::payout::UpdatePayoutStatusRequest,
) -> Result<crate::payout::PayoutLedgerEntry> {
    let api_url = booking_api_url();
    let audience = booking_api_audience();
    let url = format!("{}/api/v1/admin/ledger/{}/status", api_url, id);

    let res = get_client()
        .patch(&url, &audience, req)
        .await
        .context("Failed to connect to booking service")?;

    if !res.status().is_success() {
        let status = res.status();
        let err_text = res.text().await.unwrap_or_default();
        bail!("Failed to update payout status ({}): {}", status, err_text);
    }

    res.json::<crate::payout::PayoutLedgerEntry>()
        .await
        .context("Failed to parse updated payout entry response")
}
