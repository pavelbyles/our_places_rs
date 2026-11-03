//! Every path that decides availability has its own copy of the rule's SQL.
//! This keeps them in step: for the same booking, `check_availability`, a hold,
//! search and the calendar must all give the same answer.

use super::unavailable_ranges_tests::{HOLD_MINUTES, day, host, listing, new_booking, user};
use super::{check_availability, create_booking, get_unavailable_ranges};
use crate::error::DbError;
use crate::models::UserRole;
use chrono::{Duration, NaiveDate};
use common::models::ListingFilter;
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::path::Path;
use uuid::Uuid;

fn test_db() -> TestPg {
    dotenvy::dotenv().ok();
    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".into());
    TestPg::new(db_url, Path::new("migrations"))
}

/// A booking's state, and whether it blocks a stay.
struct State {
    label: &'static str,
    status: &'static str,
    expires_at: &'static str,
    blocks: bool,
}

const STATES: [State; 5] = [
    State {
        label: "confirmed",
        status: "confirmed",
        expires_at: "NULL",
        blocks: true,
    },
    State {
        label: "live hold",
        status: "pending",
        expires_at: "NOW() + INTERVAL '1 hour'",
        blocks: true,
    },
    State {
        label: "expired hold",
        status: "pending",
        expires_at: "NOW() - INTERVAL '1 hour'",
        blocks: false,
    },
    State {
        label: "cancelled",
        status: "cancelled",
        expires_at: "NULL",
        blocks: false,
    },
    State {
        label: "completed",
        status: "completed",
        expires_at: "NULL",
        blocks: false,
    },
];

/// The value stored on the listing, and the gap it should act as.
const GAPS: [(i32, i64); 5] = [(0, 0), (1, 1), (3, 3), (-1, 0), (i32::MAX, 365)];

async fn seed_booking(
    pool: &PgPool,
    guest: Uuid,
    villa: Uuid,
    from: NaiveDate,
    to: NaiveDate,
    state: &State,
) {
    sqlx::query(&format!(
        r#"
        INSERT INTO booking (
            id, confirmation_code, guest_id, listing_id, status,
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown,
            total_price, cancellation_policy, metadata,
            cleaning_fee, damage_deposit, platform_fee, tax_rate, exchange_rate,
            created_at, updated_at, expires_at
        )
        VALUES (
            $1, $2, $3, $4, '{status}'::booking_status,
            $5, $6, 'USD', 200.00, 2, ($6 - $5),
            400.00, 0.00, 60.00, '[]'::jsonb,
            460.00, 'flexible'::cancellation_policy, '{{"num_adults": 2}}'::jsonb,
            0.00, 0.00, 20.00, 0.1500, 1.000000,
            NOW(), NOW(), {expires}
        )
        "#,
        status = state.status,
        expires = state.expires_at,
    ))
    .bind(Uuid::now_v7())
    .bind(format!("P{}", Uuid::new_v4().simple())[..10].to_uppercase())
    .bind(guest)
    .bind(villa)
    .bind(from)
    .bind(to)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn every_path_agrees_on_which_stays_are_free() {
    let test_db = test_db();
    let pool = test_db.get_pool().await;
    let host = host(&pool).await;
    let owner = user(&pool, UserRole::Booker).await;
    let (booked_from, booked_to) = (day(8, 10), day(8, 15));
    let mut checked = 0;

    for state in &STATES {
        for (stored, gap) in GAPS {
            let gap_days = Duration::days(gap);
            let nights = Duration::days(2);
            let check_in_after = booked_to + gap_days;
            let check_out_before = booked_from - gap_days;
            let mut stays = vec![
                ("overlap", day(8, 12), day(8, 14), false),
                (
                    "just free after",
                    check_in_after,
                    check_in_after + nights,
                    true,
                ),
                (
                    "just free before",
                    check_out_before - nights,
                    check_out_before,
                    true,
                ),
            ];
            let one_day = Duration::days(1);
            stays.push((
                "one day short after",
                check_in_after - one_day,
                check_in_after - one_day + nights,
                false,
            ));
            stays.push((
                "one day short before",
                check_out_before + one_day - nights,
                check_out_before + one_day,
                false,
            ));

            for (position, from, to, free_if_blocking) in stays {
                let free = !state.blocks || free_if_blocking;
                let case = format!("{} with stored gap {stored}, stay {position}", state.label);

                // A fresh listing per case, because a successful hold probe stays.
                let villa = listing(&pool, host).await;
                sqlx::query("UPDATE listing SET days_between_bookings = $1 WHERE id = $2")
                    .bind(stored)
                    .bind(villa)
                    .execute(&pool)
                    .await
                    .unwrap();
                seed_booking(&pool, owner, villa, booked_from, booked_to, state).await;

                let check = check_availability(&pool, villa, from, to).await.unwrap();
                assert_eq!(check, free, "check_availability: {case}");

                let calendar = get_unavailable_ranges(&pool, villa, from, to)
                    .await
                    .unwrap();
                assert_eq!(calendar.is_empty(), free, "calendar: {case}");

                let filter = ListingFilter {
                    user_id: Some(host),
                    check_in: Some(from),
                    check_out: Some(to),
                    ..Default::default()
                };
                let search = crate::listing::get_listings(&pool, 1, 100, Some(filter))
                    .await
                    .unwrap();
                assert_eq!(search.iter().any(|l| l.id == villa), free, "search: {case}");

                // A different guest, because a guest's own holds don't conflict.
                let guest = user(&pool, UserRole::Booker).await;
                let hold =
                    create_booking(&pool, &new_booking(guest, villa, from, to), HOLD_MINUTES).await;
                match (hold, free) {
                    (Ok(_), true) | (Err(DbError::DateUnavailable), false) => {}
                    (other, _) => panic!("hold: {case}: expected free = {free}, got {other:?}"),
                }

                // Earlier cases' listings stay in the search results, and a page
                // is 100 at most, so keep them out of the next search's way.
                sqlx::query("UPDATE listing SET deleted_at = NOW() WHERE id = $1")
                    .bind(villa)
                    .execute(&pool)
                    .await
                    .unwrap();
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 5 * 5 * 5);
}
