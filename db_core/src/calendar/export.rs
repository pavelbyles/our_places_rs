//! What a listing's public export feed contains (spec §1.6.2): confirmed
//! stays, live holds, manual blocks and imported blocks, with raw dates (no
//! turnover gap). Busy-only: no guest name, email, phone, note or message.

use crate::error::Result;
use chrono::{Months, NaiveDate};
use common::calendar::ics_write::{ExportEvent, ExportSource};
use sqlx::PgPool;
use uuid::Uuid;

/// The listing's name, slug and export events touching `[today, today + 24
/// months]`, or `None` for an unknown or deleted listing. Inactive listings
/// still export.
#[tracing::instrument(skip(pool))]
pub async fn export_events(
    pool: &PgPool,
    listing_id: Uuid,
    today: NaiveDate,
) -> Result<Option<(String, String, Vec<ExportEvent>)>> {
    let Some(listing) = sqlx::query!(
        "SELECT name, slug FROM listing WHERE id = $1 AND deleted_at IS NULL",
        listing_id
    )
    .fetch_optional(pool)
    .await?
    else {
        return Ok(None);
    };
    let horizon = today
        .checked_add_months(Months::new(24))
        .unwrap_or(NaiveDate::MAX);
    let rows = sqlx::query!(
        r#"SELECT 'booking' AS "kind!", b.id AS "id!", NULL::text AS uid,
                b.date_from AS "date_from!", b.date_to AS "date_to!",
                b.confirmation_code AS label
            FROM booking b
            WHERE b.listing_id = $1
              AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > now()))
              AND b.date_to > $2 AND b.date_from <= $3
        UNION ALL
        SELECT CASE WHEN k.source = 'manual' THEN 'manual' ELSE 'imported' END, k.id,
                k.external_uid, k.date_from, k.date_to, f.name::text
            FROM listing_blocked_dates k
            LEFT JOIN listing_calendar_feed f ON f.id = k.feed_id
            WHERE k.listing_id = $1 AND k.date_to > $2 AND k.date_from <= $3
        ORDER BY 4, 5, 2"#,
        listing_id,
        today,
        horizon
    )
    .fetch_all(pool)
    .await?;
    let events = rows
        .into_iter()
        .map(|r| match r.kind.as_str() {
            "booking" => ExportEvent {
                uid: format!("booking-{}@ourplaces", r.id),
                date_from: r.date_from,
                date_to: r.date_to,
                source: ExportSource::Booking {
                    confirmation_code: r.label.unwrap_or_default(),
                },
            },
            "manual" => ExportEvent {
                uid: format!("block-{}@ourplaces", r.id),
                date_from: r.date_from,
                date_to: r.date_to,
                source: ExportSource::Manual,
            },
            _ => ExportEvent {
                uid: r.uid.unwrap_or_else(|| format!("block-{}@ourplaces", r.id)),
                date_from: r.date_from,
                date_to: r.date_to,
                source: ExportSource::Imported {
                    feed_name: r.label.unwrap_or_default(),
                },
            },
        })
        .collect();
    Ok(Some((listing.name, listing.slug, events)))
}
