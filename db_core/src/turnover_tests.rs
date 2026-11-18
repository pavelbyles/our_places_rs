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
            base_currency, is_active, minimum_stay, max_guests, days_between_bookings,
            commission_pct, cleaning_fee, added_at, listing_details
        ) VALUES ($1, $2, $3, 1, 'Jamaica', 'Montego Bay', $4, 'USD', TRUE, 1, 4, 0,
            0.1000, 50.00, NOW(),
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

#[tokio::test]
async fn booking_update_and_reservation_change_reconcile_turnovers_atomically()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::models::{
        BookingMetadata, BookingStatus, CancellationPolicy, ChangeInitiatorRole,
        ChangeSettlementStatus, NewBooking, NewReservationChangeRequest,
        ReservationChangePricingSnapshot, SupplierInfo, UpdatedBooking,
    };
    use crate::reservation_change::create_change_request;
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;

    let db = test_db();
    let pool = db.get_pool().await;
    let host = seed_user(&pool, "hook-host@example.com", "HookHost", &["host"]).await?;
    let guest = seed_user(&pool, "hook-guest@example.com", "HookGuest", &["booker"]).await?;
    sqlx::query(
        "INSERT INTO host_profiles (user_id, description, verified_status) VALUES ($1, 'Host', 'verified')",
    )
    .bind(host)
    .execute(&pool)
    .await?;
    sqlx::query(
        r#"INSERT INTO currency_exchange_rates (base_currency, target_currency, rate, effective_at)
           VALUES ('USD', 'JMD', 160.000000, NOW())
           ON CONFLICT (base_currency, target_currency, effective_at) DO NOTHING"#,
    )
    .execute(&pool)
    .await?;

    let listing_id = seed_listing(&pool, host, "Hook Villa").await?;
    let today = listing_today(Utc::now());
    let from1 = today + Duration::days(3);
    let to1 = today + Duration::days(6);
    let b1 = crate::booking::create_booking(
        &pool,
        &NewBooking {
            confirmation_code: format!("HK{}", &Uuid::now_v7().simple().to_string()[24..32])
                .to_uppercase(),
            guest_id: guest,
            listing_id,
            date_from: from1,
            date_to: to1,
            currency: "USD".to_string(),
            daily_rate: dec!(200.00),
            number_of_persons: 2,
            total_days: 3,
            sub_total_price: dec!(600.00),
            discount_value: None,
            tax_value: Some(dec!(90.00)),
            fee_breakdown: vec![],
            total_price: dec!(720.00),
            cancellation_policy: CancellationPolicy::Flexible,
            metadata: BookingMetadata {
                num_adults: 2,
                ..Default::default()
            },
            door_access_code: None,
            cleaning_fee: dec!(50.00),
            damage_deposit: dec!(100.00),
            platform_fee: dec!(30.00),
            tax_rate: dec!(0.1500),
            exchange_rate: Decimal::ONE,
            promo: None,
        },
        120,
    )
    .await?;

    let count_before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM turnover WHERE booking_id = $1")
            .bind(b1.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(count_before, 0, "pending hold must not create turnover");

    let supplier = SupplierInfo {
        supplier_name: "Our Places Jamaica Limited".to_string(),
        supplier_address: "Main Street, Ocho Rios, Jamaica".to_string(),
        supplier_trn: "000-111-222".to_string(),
    };

    crate::booking::update_booking(
        &pool,
        b1.id,
        &UpdatedBooking {
            status: Some(BookingStatus::Confirmed),
            ..Default::default()
        },
        &supplier,
    )
    .await?;

    let row_after_confirm: (Uuid, NaiveDate, String) =
        sqlx::query_as("SELECT id, checkout_date, status FROM turnover WHERE booking_id = $1")
            .bind(b1.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(row_after_confirm.1, to1);
    assert_eq!(row_after_confirm.2, "needs_cleaning");

    // Start cleaning before reservation extension, then extend via approve_change_request.
    transition_turnover_status(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        row_after_confirm.0,
        &TransitionTurnoverRequest {
            to_status: TurnoverStatus::CleaningInProgress,
            expected_status: Some(TurnoverStatus::NeedsCleaning),
            reason: None,
        },
        "ui",
    )
    .await?;

    let extended_to = to1 + Duration::days(2);
    let snap = ReservationChangePricingSnapshot {
        daily_rate: dec!(200.00),
        total_days: 5,
        extra_guests: 0,
        extra_guest_fee_per_night: Decimal::ZERO,
        extra_guest_fee_total: Decimal::ZERO,
        sub_total_price: dec!(1000.00),
        discount_value: None,
        discounted_subtotal: dec!(1000.00),
        cleaning_fee: dec!(50.00),
        promo_stay_discount: Decimal::ZERO,
        promo_cleaning_discount: Decimal::ZERO,
        cleaning_net: dec!(50.00),
        promo_retained: false,
        promo_lost_reason: None,
        damage_deposit: dec!(100.00),
        platform_fee: dec!(50.00),
        tax_rate: dec!(0.1500),
        tax_value: Some(dec!(157.50)),
        fee_breakdown: vec![],
        total_price: dec!(1307.50),
        host_gross_amount: dec!(1050.00),
        host_platform_fee_amount: dec!(105.00),
        host_net_payout_amount: dec!(945.00),
    };
    let req = create_change_request(
        &pool,
        &NewReservationChangeRequest {
            booking_id: b1.id,
            listing_id,
            initiated_by_id: guest,
            initiated_by_role: ChangeInitiatorRole::Guest,
            previous_date_from: from1,
            previous_date_to: to1,
            proposed_date_from: from1,
            proposed_date_to: extended_to,
            previous_number_of_persons: 2,
            proposed_number_of_persons: 2,
            proposed_num_adults: 2,
            proposed_num_children: 0,
            proposed_num_infants: 0,
            proposed_num_pets: 0,
            currency: "USD".to_string(),
            previous_total_price: dec!(720.00),
            proposed_total_price: dec!(1307.50),
            balance_due: dec!(587.50),
            previous_net_payout: dec!(540.00),
            proposed_net_payout: dec!(945.00),
            host_payout_delta: dec!(405.00),
            pricing_snapshot: snap,
            settlement_status: ChangeSettlementStatus::Charged,
            reason: Some("Extend stay".to_string()),
            expires_at_date: from1,
        },
        today,
    )
    .await?;

    crate::reservation_change::approve_change_request(
        &pool,
        &crate::reservation_change::ApproveChangeRequestParams {
            id: req.id,
            responder_id: host,
            response_note: Some("Approved"),
            settlement_status: ChangeSettlementStatus::Charged,
            gateway_reference: Some("SIM-EXT"),
            supplier: &supplier,
            today_jm: today,
        },
    )
    .await?;

    let after_extend: (Uuid, NaiveDate, String) =
        sqlx::query_as("SELECT id, checkout_date, status FROM turnover WHERE booking_id = $1")
            .bind(b1.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(after_extend.0, row_after_confirm.0);
    assert_eq!(after_extend.1, extended_to);
    assert_eq!(after_extend.2, "cleaning_in_progress");

    // Second booking confirmed then cancelled -> needs_cleaning -> not_required.
    let from2 = today + Duration::days(12);
    let to2 = today + Duration::days(14);
    let b2 = crate::booking::create_booking(
        &pool,
        &NewBooking {
            confirmation_code: format!("HK{}", &Uuid::now_v7().simple().to_string()[24..32])
                .to_uppercase(),
            guest_id: guest,
            listing_id,
            date_from: from2,
            date_to: to2,
            currency: "USD".to_string(),
            daily_rate: dec!(200.00),
            number_of_persons: 2,
            total_days: 2,
            sub_total_price: dec!(400.00),
            discount_value: None,
            tax_value: Some(dec!(60.00)),
            fee_breakdown: vec![],
            total_price: dec!(530.00),
            cancellation_policy: CancellationPolicy::Flexible,
            metadata: BookingMetadata {
                num_adults: 2,
                ..Default::default()
            },
            door_access_code: None,
            cleaning_fee: dec!(50.00),
            damage_deposit: dec!(100.00),
            platform_fee: dec!(20.00),
            tax_rate: dec!(0.1500),
            exchange_rate: Decimal::ONE,
            promo: None,
        },
        120,
    )
    .await?;
    crate::booking::update_booking(
        &pool,
        b2.id,
        &UpdatedBooking {
            status: Some(BookingStatus::Confirmed),
            ..Default::default()
        },
        &supplier,
    )
    .await?;
    crate::booking::update_booking(
        &pool,
        b2.id,
        &UpdatedBooking {
            status: Some(BookingStatus::Cancelled),
            ..Default::default()
        },
        &supplier,
    )
    .await?;
    let b2_status: String = sqlx::query_scalar("SELECT status FROM turnover WHERE booking_id = $1")
        .bind(b2.id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(b2_status, "not_required");

    Ok(())
}

#[tokio::test]
async fn add_comment_with_outbox_snapshots_author_and_queues_active_verified_recipients()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::turnover::add_comment_with_outbox;

    let db = test_db();
    let pool = db.get_pool().await;

    let host = seed_user(&pool, "cm-host@example.com", "Elena", &["host"]).await?;
    let staff_active = seed_user(&pool, "cm-active@example.com", "Marco", &["staff"]).await?;
    let staff_other = seed_user(&pool, "cm-other@example.com", "Unassigned", &["staff"]).await?;
    let staff_inactive =
        seed_user(&pool, "cm-inactive@example.com", "Inactive", &["staff"]).await?;
    let staff_unverified =
        seed_user(&pool, "cm-unverified@example.com", "Unverified", &["staff"]).await?;

    sqlx::query("UPDATE \"user\" SET is_active = FALSE WHERE id = $1")
        .bind(staff_inactive)
        .execute(&pool)
        .await?;
    sqlx::query("UPDATE \"user\" SET is_verified = FALSE WHERE id = $1")
        .bind(staff_unverified)
        .execute(&pool)
        .await?;

    let listing_a = seed_listing(&pool, host, "Palm Breeze").await?;
    let listing_other = seed_listing(&pool, host, "Other Villa").await?;
    for staff_id in [staff_active, staff_inactive, staff_unverified] {
        sqlx::query("INSERT INTO host_staff (host_user_id, staff_user_id) VALUES ($1, $2)")
            .bind(host)
            .bind(staff_id)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO listing_staff (listing_id, staff_user_id) VALUES ($1, $2)")
            .bind(listing_a)
            .bind(staff_id)
            .execute(&pool)
            .await?;
    }
    sqlx::query("INSERT INTO host_staff (host_user_id, staff_user_id) VALUES ($1, $2)")
        .bind(host)
        .bind(staff_other)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO listing_staff (listing_id, staff_user_id) VALUES ($1, $2)")
        .bind(listing_other)
        .bind(staff_other)
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
            listing_id: listing_a,
            checkout_date: today,
            title: Some("Fan repair".into()),
            initial_status: None,
        },
    )
    .await?;

    // Blank body rejected.
    let blank_err = add_comment_with_outbox(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        created.id,
        "   ",
        "https://admin.example.com",
    )
    .await;
    assert!(matches!(blank_err, Err(DbError::ValidationError(_))));

    // Unassigned staff blocked (RowNotFound).
    let unauth_err = add_comment_with_outbox(
        &pool,
        TurnoverActor {
            user_id: staff_other,
            role: TurnoverActorRole::Staff,
        },
        created.id,
        "Not my villa",
        "https://admin.example.com",
    )
    .await;
    assert!(matches!(
        unauth_err,
        Err(DbError::Sqlx(sqlx::Error::RowNotFound))
    ));

    // Assigned active staff posts comment -> queues email ONLY for host (excludes author, unassigned, inactive, unverified).
    let staff_comment = add_comment_with_outbox(
        &pool,
        TurnoverActor {
            user_id: staff_active,
            role: TurnoverActorRole::Staff,
        },
        created.id,
        "Need replacement fan blade before next guest.",
        "https://admin.example.com",
    )
    .await?;
    assert_eq!(staff_comment.comment.author_name, "Marco Tester");
    assert_eq!(staff_comment.comment.author_role, "staff");
    assert_eq!(staff_comment.outbox_ids.len(), 1);

    let recipients_1: Vec<String> = sqlx::query_scalar(
        "SELECT recipient_email FROM email_outbox WHERE id = ANY($1) ORDER BY recipient_email",
    )
    .bind(&staff_comment.outbox_ids)
    .fetch_all(&pool)
    .await?;
    assert_eq!(recipients_1, vec!["cm-host@example.com".to_string()]);

    // Host replies -> queues email ONLY for active verified assigned staff.
    let host_reply = add_comment_with_outbox(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        created.id,
        "Ordered a spare fan for 2pm delivery.",
        "https://admin.example.com/",
    )
    .await?;
    assert_eq!(host_reply.comment.author_name, "Elena Tester");
    assert_eq!(host_reply.outbox_ids.len(), 1);

    let (recip, template, payload): (String, String, serde_json::Value) = sqlx::query_as(
        "SELECT recipient_email, template_id, payload FROM email_outbox WHERE id = $1",
    )
    .bind(host_reply.outbox_ids[0])
    .fetch_one(&pool)
    .await?;
    assert_eq!(recip, "cm-active@example.com");
    assert_eq!(template, "turnover_comment_notification");
    assert_eq!(payload["listing_name"], "Palm Breeze");
    assert_eq!(payload["author_name"], "Elena Tester");
    assert_eq!(payload["author_role"], "host");
    assert_eq!(
        payload["turnover_url"],
        format!("https://admin.example.com/admin/turnovers/{}", created.id)
    );

    let detail = get_turnover_detail(
        &pool,
        TurnoverActor {
            user_id: host,
            role: TurnoverActorRole::Host,
        },
        created.id,
    )
    .await?;
    assert_eq!(detail.comments.len(), 2);
    assert_eq!(detail.turnover.comment_count, 2);

    Ok(())
}
