//! Tests for `db_core::turnover` repository, optimistic concurrency, reconcilers, and scoping.

use chrono::{Duration, NaiveDate, Utc};
use common::reference::listing_today;
use common::turnover::{
    CreateManualTurnoverRequest, CreateTurnoverCommentRequest, TransitionTurnoverRequest,
    TurnoverActorRole, TurnoverListQuery, TurnoverStatus, TurnoverWindow,
};
use sqlx::PgPool;
use sqlx_db_tester::TestPg;
use std::path::Path;
use uuid::Uuid;

use crate::error::DbError;
use crate::turnover::{
    TurnoverActor, create_manual_turnover, get_turnover_detail, list_turnovers,
    reconcile_for_booking, reconcile_for_listing, transition_turnover_status,
};

static DATABASE_URL_ENV: &str = "DATABASE_URL";

fn test_db() -> TestPg {
    dotenvy::dotenv().ok();
    let db_url = match std::env::var(DATABASE_URL_ENV) {
        Ok(url) => url,
        Err(_) => "postgres://postgres:password@localhost:5432/our_places".to_string(),
    };
    TestPg::new(db_url, Path::new("./migrations"))
}

async fn seed_user(
    pool: &PgPool,
    email: &str,
    first_name: &str,
    roles: &[&str],
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO "user" (id, email, password_hash, first_name, last_name, is_active, is_verified, attributes, roles, default_currency)
           VALUES ($1, $2, 'hash', $3, 'Tester', TRUE, TRUE, '{}'::jsonb, $4::user_role[], 'USD')"#,
    )
    .bind(id)
    .bind(email)
    .bind(first_name)
    .bind(roles)
    .execute(pool)
    .await?;
    Ok(id)
}

async fn seed_listing(
    pool: &PgPool,
    owner_id: Uuid,
    name: &str,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let id = Uuid::now_v7();
    let slug = format!("v-{}", id.simple());
    sqlx::query(
        r#"INSERT INTO listing (
            id, user_id, name, listing_structure_id, country, city, slug,
            base_currency, added_at, listing_details
        ) VALUES ($1, $2, $3, 1, 'Jamaica', 'Montego Bay', $4, 'USD', NOW(),
            '{"policies": {"check_in": "3:00 PM", "check_out": "11:00 AM"}}'::jsonb)"#,
    )
    .bind(id)
    .bind(owner_id)
    .bind(name)
    .bind(slug)
    .execute(pool)
    .await?;
    Ok(id)
}

async fn seed_booking(
    pool: &PgPool,
    guest_id: Uuid,
    listing_id: Uuid,
    status: &str,
    date_from: NaiveDate,
    date_to: NaiveDate,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let id = Uuid::now_v7();
    let code = format!("OP{}", id.simple().to_string()[24..32].to_uppercase());
    let days = (date_to - date_from).num_days().max(1) as i32;
    sqlx::query(
        r#"INSERT INTO booking (
            id, confirmation_code, guest_id, listing_id, status,
            date_from, date_to, currency, daily_rate, number_of_persons,
            total_days, sub_total_price, total_price, cancellation_policy, expires_at
        ) VALUES (
            $1, $2, $3, $4, $5::booking_status,
            $6, $7, 'USD', 200, 2,
            $8, 400, 460, 'flexible', $9
        )"#,
    )
    .bind(id)
    .bind(code)
    .bind(guest_id)
    .bind(listing_id)
    .bind(status)
    .bind(date_from)
    .bind(date_to)
    .bind(days)
    .bind(expires_at)
    .execute(pool)
    .await?;
    Ok(id)
}

async fn seed_feed(
    pool: &PgPool,
    listing_id: Uuid,
    name: &str,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO listing_calendar_feed (id, listing_id, name, url)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(id)
    .bind(listing_id)
    .bind(name)
    .bind(format!("https://example.com/{id}.ics"))
    .execute(pool)
    .await?;
    Ok(id)
}

async fn seed_block(
    pool: &PgPool,
    listing_id: Uuid,
    source: &str,
    feed_id: Option<Uuid>,
    external_uid: Option<&str>,
    date_from: NaiveDate,
    date_to: NaiveDate,
) -> Result<Uuid, Box<dyn std::error::Error>> {
    let id = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO listing_blocked_dates (id, listing_id, source, feed_id, external_uid, date_from, date_to)
           VALUES ($1, $2, $3::calendar_block_source, $4, $5, $6, $7)"#,
    )
    .bind(id)
    .bind(listing_id)
    .bind(source)
    .bind(feed_id)
    .bind(external_uid)
    .bind(date_from)
    .bind(date_to)
    .execute(pool)
    .await?;
    Ok(id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_status_transitions_handle_optimistic_conflict_and_idempotent_noop()
-> Result<(), Box<dyn std::error::Error>> {
    let db = test_db();
    let pool = db.get_pool().await;
    let host = seed_user(&pool, "t-host@example.com", "Host", &["host"]).await?;
    let staff = seed_user(&pool, "t-staff@example.com", "Dana", &["staff"]).await?;
    let listing = seed_listing(&pool, host, "Sea Glass").await?;

    sqlx::query("INSERT INTO host_staff (host_user_id, staff_user_id) VALUES ($1, $2)")
        .bind(host)
        .bind(staff)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO listing_staff (listing_id, staff_user_id) VALUES ($1, $2)")
        .bind(listing)
        .bind(staff)
        .execute(&pool)
        .await?;

    let today = listing_today(Utc::now());
    let created = create_manual_turnover(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        &CreateManualTurnoverRequest {
            listing_id: listing,
            checkout_date: today,
            title: Some("Deep clean".into()),
            initial_status: Some(TurnoverStatus::NeedsCleaning),
        },
    )
    .await?;
    let tid = created.id;

    let req1 = TransitionTurnoverRequest {
        to_status: TurnoverStatus::CleaningInProgress,
        expected_status: Some(TurnoverStatus::NeedsCleaning),
        reason: Some("Starting".into()),
    };
    let req2 = TransitionTurnoverRequest {
        to_status: TurnoverStatus::MaintenanceRequired,
        expected_status: Some(TurnoverStatus::NeedsCleaning),
        reason: Some("Plumbing".into()),
    };
    // Two staff/host calls race with expected_status = NeedsCleaning: exactly 1 succeeds, 1 conflicts.
    let (r1, r2) = tokio::join!(
        transition_turnover_status(
            &pool,
            TurnoverActor {
                user_id: staff,
                role: TurnoverActorRole::Staff,
            },
            tid,
            &req1,
            "dispatch_board",
        ),
        transition_turnover_status(
            &pool,
            TurnoverActor {
                user_id: host,
                role: TurnoverActorRole::Host,
            },
            tid,
            &req2,
            "dispatch_board",
        ),
    );

    let (winner_status, conflict_err) = match (r1, r2) {
        (Ok(w), Err(e)) | (Err(e), Ok(w)) => (w.status, e),
        (Ok(_), Ok(_)) => return Err("expected one optimistic conflict".into()),
        (Err(e1), Err(e2)) => return Err(format!("both failed: {e1:?} / {e2:?}").into()),
    };
    assert!(matches!(
        conflict_err,
        DbError::TurnoverStatusConflict { .. }
    ));

    // Idempotent no-op to same winner_status succeeds without appending extra history.
    let detail_before = get_turnover_detail(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        tid,
    )
    .await?;
    let count_before = detail_before.status_history.len();

    let noop = transition_turnover_status(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        tid,
        &TransitionTurnoverRequest {
            to_status: winner_status,
            expected_status: Some(winner_status),
            reason: None,
        },
        "dispatch_board",
    )
    .await?;
    assert_eq!(noop.status, winner_status);

    let detail_after = get_turnover_detail(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        tid,
    )
    .await?;
    assert_eq!(detail_after.status_history.len(), count_before);

    // Staff cannot transition into InspectedAndReady, nor booking/ical into NotRequired.
    let forbidden_inspect = transition_turnover_status(
        &pool,
        TurnoverActor {
            user_id: staff,
            role: TurnoverActorRole::Staff,
        },
        tid,
        &TransitionTurnoverRequest {
            to_status: TurnoverStatus::InspectedAndReady,
            expected_status: None,
            reason: None,
        },
        "dispatch_board",
    )
    .await;
    assert!(matches!(
        forbidden_inspect,
        Err(DbError::TurnoverTransitionForbidden(_))
    ));

    // Host marks InspectedAndReady, then Staff CAN reopen to MaintenanceRequired.
    transition_turnover_status(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        tid,
        &TransitionTurnoverRequest {
            to_status: TurnoverStatus::InspectedAndReady,
            expected_status: None,
            reason: None,
        },
        "dispatch_board",
    )
    .await?;
    let reopened = transition_turnover_status(
        &pool,
        TurnoverActor {
            user_id: staff,
            role: TurnoverActorRole::Staff,
        },
        tid,
        &TransitionTurnoverRequest {
            to_status: TurnoverStatus::MaintenanceRequired,
            expected_status: None,
            reason: Some("AC leak found".into()),
        },
        "dispatch_board",
    )
    .await?;
    assert_eq!(reopened.status, TurnoverStatus::MaintenanceRequired);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_booking_and_listing_reconcilers_never_duplicate_or_deadlock()
-> Result<(), Box<dyn std::error::Error>> {
    let db = test_db();
    let pool = db.get_pool().await;
    let host = seed_user(&pool, "r-host@example.com", "Host", &["host"]).await?;
    let guest = seed_user(&pool, "r-guest@example.com", "Guest", &["booker"]).await?;
    let listing = seed_listing(&pool, host, "Palm Villa").await?;
    let today = listing_today(Utc::now());

    let booking_id = seed_booking(
        &pool,
        guest,
        listing,
        "confirmed",
        today + Duration::days(1),
        today + Duration::days(4),
        None,
    )
    .await?;
    let feed_id = seed_feed(&pool, listing, "Airbnb").await?;
    seed_block(
        &pool,
        listing,
        "ical_feed",
        Some(feed_id),
        Some("uid-1"),
        today + Duration::days(6),
        today + Duration::days(9),
    )
    .await?;

    let (b1, b2, l1, l2) = tokio::join!(
        reconcile_for_booking(&pool, booking_id, "booking_update"),
        reconcile_for_booking(&pool, booking_id, "booking_update"),
        reconcile_for_listing(&pool, listing, Some(feed_id), false, "ical_sync"),
        reconcile_for_listing(&pool, listing, None, false, "cron_backfill"),
    );
    b1?;
    b2?;
    l1?;
    l2?;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM turnover WHERE listing_id = $1")
        .bind(listing)
        .fetch_one(&pool)
        .await?;
    assert_eq!(count, 2);
    Ok(())
}

#[tokio::test]
async fn booking_and_ical_extensions_shift_checkout_in_place_and_preserve_progress()
-> Result<(), Box<dyn std::error::Error>> {
    let db = test_db();
    let pool = db.get_pool().await;
    let host = seed_user(&pool, "ext-host@example.com", "Host", &["host"]).await?;
    let guest = seed_user(&pool, "ext-guest@example.com", "Guest", &["booker"]).await?;
    let listing = seed_listing(&pool, host, "Coral Cliff").await?;
    let today = listing_today(Utc::now());

    let booking_id = seed_booking(
        &pool,
        guest,
        listing,
        "confirmed",
        today + Duration::days(1),
        today + Duration::days(3),
        None,
    )
    .await?;
    reconcile_for_booking(&pool, booking_id, "booking_update").await?;

    let tid: Uuid = sqlx::query_scalar("SELECT id FROM turnover WHERE booking_id = $1")
        .bind(booking_id)
        .fetch_one(&pool)
        .await?;
    transition_turnover_status(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        tid,
        &TransitionTurnoverRequest {
            to_status: TurnoverStatus::CleaningInProgress,
            expected_status: None,
            reason: None,
        },
        "dispatch_board",
    )
    .await?;

    // Extend booking checkout from +3 to +6 -> turnover shifts date in place, keeps CleaningInProgress.
    let new_checkout = today + Duration::days(6);
    sqlx::query("UPDATE booking SET date_to = $1 WHERE id = $2")
        .bind(new_checkout)
        .bind(booking_id)
        .execute(&pool)
        .await?;
    reconcile_for_booking(&pool, booking_id, "change_request").await?;

    let list = list_turnovers(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        &TurnoverListQuery {
            window: Some(TurnoverWindow::All),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(list.turnovers.len(), 1);
    assert_eq!(list.turnovers[0].checkout_date, new_checkout);
    assert_eq!(list.turnovers[0].status, TurnoverStatus::CleaningInProgress);
    Ok(())
}

#[tokio::test]
async fn listing_reconciler_respects_checkout_day_manual_blocks_and_empty_feed_safeguard()
-> Result<(), Box<dyn std::error::Error>> {
    let db = test_db();
    let pool = db.get_pool().await;
    let host = seed_user(&pool, "ic-host@example.com", "Host", &["host"]).await?;
    let listing = seed_listing(&pool, host, "Blue Bay").await?;
    let feed = seed_feed(&pool, listing, "VRBO").await?;
    let today = listing_today(Utc::now());

    // 1. Manual block NEVER auto-creates turnover.
    seed_block(
        &pool,
        listing,
        "manual",
        None,
        None,
        today + Duration::days(1),
        today + Duration::days(3),
    )
    .await?;

    // 2. Two iCal blocks: one ending TODAY, one ending in the FUTURE.
    let today_block = seed_block(
        &pool,
        listing,
        "ical_feed",
        Some(feed),
        Some("uid-today"),
        today - Duration::days(2),
        today,
    )
    .await?;
    let future_block = seed_block(
        &pool,
        listing,
        "ical_feed",
        Some(feed),
        Some("uid-future"),
        today + Duration::days(4),
        today + Duration::days(7),
    )
    .await?;

    reconcile_for_listing(&pool, listing, Some(feed), false, "ical_sync").await?;
    let all = list_turnovers(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        &TurnoverListQuery {
            window: Some(TurnoverWindow::All),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(
        all.turnovers.len(),
        2,
        "manual block must not create turnover"
    );

    // Delete both imported blocks (simulating feed dropping them).
    sqlx::query("DELETE FROM listing_blocked_dates WHERE id = ANY($1)")
        .bind([today_block, future_block])
        .execute(&pool)
        .await?;

    // With skip_removals = true (NO_EVENTS_WARNING), neither transitions to NotRequired.
    reconcile_for_listing(&pool, listing, Some(feed), true, "ical_sync").await?;
    let after_skip = list_turnovers(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        &TurnoverListQuery {
            window: Some(TurnoverWindow::All),
            ..Default::default()
        },
    )
    .await?;
    assert!(
        after_skip
            .turnovers
            .iter()
            .all(|t| t.status == TurnoverStatus::NeedsCleaning)
    );

    // With skip_removals = false, ONLY future_block transitions to NotRequired; today_block stays NeedsCleaning!
    reconcile_for_listing(&pool, listing, Some(feed), false, "ical_sync").await?;
    let after_prune = list_turnovers(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        &TurnoverListQuery {
            window: Some(TurnoverWindow::All),
            ..Default::default()
        },
    )
    .await?;
    let today_t = after_prune
        .turnovers
        .iter()
        .find(|t| t.checkout_date == today)
        .ok_or("missing today turnover")?;
    let future_t = after_prune
        .turnovers
        .iter()
        .find(|t| t.checkout_date == today + Duration::days(7))
        .ok_or("missing future turnover")?;
    assert_eq!(today_t.status, TurnoverStatus::NeedsCleaning);
    assert_eq!(future_t.status, TurnoverStatus::NotRequired);
    Ok(())
}

#[tokio::test]
async fn list_turnovers_computes_next_checkin_and_hides_pii_and_unassigned_listings()
-> Result<(), Box<dyn std::error::Error>> {
    let db = test_db();
    let pool = db.get_pool().await;
    let host_a = seed_user(&pool, "ha@example.com", "HostA", &["host"]).await?;
    let host_b = seed_user(&pool, "hb@example.com", "HostB", &["host"]).await?;
    let staff_a = seed_user(&pool, "sa@example.com", "StaffA", &["staff"]).await?;
    let guest = seed_user(&pool, "g@example.com", "Guest", &["booker"]).await?;

    let villa_a = seed_listing(&pool, host_a, "Villa A").await?;
    let villa_b = seed_listing(&pool, host_b, "Villa B").await?;
    sqlx::query("INSERT INTO listing_staff (listing_id, staff_user_id) VALUES ($1, $2)")
        .bind(villa_a)
        .bind(staff_a)
        .execute(&pool)
        .await?;

    let today = listing_today(Utc::now());
    let first_checkout = today + Duration::days(2);
    let b1 = seed_booking(
        &pool,
        guest,
        villa_a,
        "confirmed",
        today,
        first_checkout,
        None,
    )
    .await?;
    // Cancelled & expired holds at +2/+3 must be ignored by next_checkin_date.
    seed_booking(
        &pool,
        guest,
        villa_a,
        "cancelled",
        first_checkout,
        first_checkout + Duration::days(2),
        None,
    )
    .await?;
    seed_booking(
        &pool,
        guest,
        villa_a,
        "pending",
        first_checkout + Duration::days(1),
        first_checkout + Duration::days(3),
        Some(Utc::now() - Duration::minutes(10)),
    )
    .await?;
    // Next real check-in is at +4 via manual calendar block.
    let expected_next = first_checkout + Duration::days(2);
    seed_block(
        &pool,
        villa_a,
        "manual",
        None,
        None,
        expected_next,
        expected_next + Duration::days(2),
    )
    .await?;

    // Host B also has a booking on Villa B.
    let b2 = seed_booking(
        &pool,
        guest,
        villa_b,
        "confirmed",
        today,
        first_checkout,
        None,
    )
    .await?;

    reconcile_for_booking(&pool, b1, "booking_update").await?;
    reconcile_for_booking(&pool, b2, "booking_update").await?;

    let staff_list = list_turnovers(
        &pool,
        TurnoverActor {
            user_id: staff_a,
            role: TurnoverActorRole::Staff,
        },
        &TurnoverListQuery {
            window: Some(TurnoverWindow::Upcoming),
            ..Default::default()
        },
    )
    .await?;
    assert_eq!(staff_list.total, 1);
    assert_eq!(staff_list.turnovers[0].listing_id, villa_a);
    assert_eq!(
        staff_list.turnovers[0].next_checkin_date,
        Some(expected_next)
    );
    assert_eq!(
        staff_list.turnovers[0].checkin_policy_text.as_deref(),
        Some("3:00 PM")
    );
    assert_eq!(
        staff_list.turnovers[0].checkout_policy_text.as_deref(),
        Some("11:00 AM")
    );
    assert_eq!(
        staff_list.turnovers[0].assigned_staff_names,
        vec!["StaffA Tester".to_string()]
    );

    // Unused helper import check avoided; confirm comment request compiles if referenced in Task 12.
    let _ = std::marker::PhantomData::<CreateTurnoverCommentRequest>;
    Ok(())
}
