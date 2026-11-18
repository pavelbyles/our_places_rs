//! Integration tests for `/api/v1/turnovers` HTTP endpoints and authorization boundaries.

use super::authz_tests::{settings, tax_settings, user};
use super::request_api_tests::bearer;
use super::stay_pricing_pin_tests::test_db;
use actix_web::{App, test, web};
use chrono::{Duration, Utc};
use common::reference::listing_today;
use common::turnover::{
    TurnoverDetailResponse, TurnoverListResponse, TurnoverResponse, TurnoverSourceKind,
    TurnoverStatus,
};
use db_core::models::UserRole;
use sqlx::PgPool;
use uuid::Uuid;

macro_rules! app {
    ($pool:expr) => {
        test::init_service(
            App::new()
                .app_data(web::Data::new($pool.clone()))
                .app_data(web::Data::new(settings()))
                .app_data(web::Data::new(tax_settings()))
                .configure(crate::apis::configure_routes),
        )
        .await
    };
}

async fn seed_listing(pool: &PgPool, host_id: Uuid, name: &str) -> Result<Uuid, sqlx::Error> {
    let listing_id = Uuid::now_v7();
    let slug = format!("tv-{}", listing_id.simple());
    sqlx::query(
        r#"INSERT INTO listing (
            id, user_id, name, listing_structure_id, country, city, slug,
            base_currency, is_active, minimum_stay, max_guests, days_between_bookings,
            commission_pct, cleaning_fee, added_at, listing_details
        ) VALUES ($1, $2, $3, 1, 'Jamaica', 'Negril', $4, 'USD', TRUE, 1, 4, 0,
            0.1000, 50.00, NOW(),
            '{"policies": {"check_in": "3:00 PM", "check_out": "11:00 AM"}}'::jsonb)"#,
    )
    .bind(listing_id)
    .bind(host_id)
    .bind(name)
    .bind(slug)
    .execute(pool)
    .await?;
    Ok(listing_id)
}

async fn link_staff(
    pool: &PgPool,
    host_id: Uuid,
    staff_id: Uuid,
    listing_id: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO host_staff (host_user_id, staff_user_id) VALUES ($1, $2)")
        .bind(host_id)
        .bind(staff_id)
        .execute(pool)
        .await?;
    sqlx::query("INSERT INTO listing_staff (listing_id, staff_user_id) VALUES ($1, $2)")
        .bind(listing_id)
        .bind(staff_id)
        .execute(pool)
        .await?;
    Ok(())
}

#[tokio::test]
async fn turnover_api_enforces_scoping_role_transitions_conflicts_and_zero_pii()
-> Result<(), Box<dyn std::error::Error>> {
    let test_db = test_db();
    let pool = test_db.get_pool().await;

    let admin = user(&pool, UserRole::Admin).await;
    let host_a = user(&pool, UserRole::Host).await;
    let host_b = user(&pool, UserRole::Host).await;
    let staff_a = user(&pool, UserRole::Staff).await;
    let booker = user(&pool, UserRole::Booker).await;

    let guest_id = Uuid::now_v7();
    let guest_email = "secret-guest-pii@example.com";
    sqlx::query(
        r#"INSERT INTO "user" (id, email, password_hash, first_name, last_name, is_active, is_verified, attributes, roles, default_currency)
           VALUES ($1, $2, 'hash', 'SecretFirstName', 'SecretLastName', TRUE, TRUE, '{}'::jsonb, ARRAY['booker']::user_role[], 'USD')"#,
    )
    .bind(guest_id)
    .bind(guest_email)
    .execute(&pool)
    .await?;

    let villa_a = seed_listing(&pool, host_a, "Coral Cove").await?;
    let villa_unassigned = seed_listing(&pool, host_a, "Hidden Ridge").await?;
    let villa_b = seed_listing(&pool, host_b, "Palm Bay").await?;
    link_staff(&pool, host_a, staff_a, villa_a).await?;

    let today = listing_today(Utc::now());
    let booking_a = Uuid::now_v7();
    let conf_code = "SECRETCONF99";
    let door_code = "DOORPIN8888";
    sqlx::query(
        r#"INSERT INTO booking (
            id, confirmation_code, guest_id, listing_id, status,
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown,
            total_price, cancellation_policy, metadata, door_access_code, cleaning_fee,
            damage_deposit, platform_fee, tax_rate, exchange_rate, created_at, updated_at
        ) VALUES (
            $1, $2, $3, $4, 'confirmed'::booking_status,
            $5, $6, 'USD', 325.00, 2, 3,
            975.00, 0.00, 146.25, '[]'::jsonb,
            1234.56, 'flexible'::cancellation_policy, '{}'::jsonb, $7, 50.00,
            100.00, 40.00, 0.1500, 1.000000, NOW(), NOW()
        )"#,
    )
    .bind(booking_a)
    .bind(conf_code)
    .bind(guest_id)
    .bind(villa_a)
    .bind(today - Duration::days(3))
    .bind(today)
    .bind(door_code)
    .execute(&pool)
    .await?;
    db_core::turnover::reconcile_for_booking(&pool, booking_a, "test_seed").await?;

    let booking_hidden = Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO booking (
            id, confirmation_code, guest_id, listing_id, status,
            date_from, date_to, currency, daily_rate, number_of_persons, total_days,
            sub_total_price, discount_value, tax_value, fee_breakdown,
            total_price, cancellation_policy, metadata, cleaning_fee,
            damage_deposit, platform_fee, tax_rate, exchange_rate, created_at, updated_at
        ) VALUES (
            $1, 'HIDDEN999', $2, $3, 'confirmed'::booking_status,
            $4, $5, 'USD', 200.00, 2, 2,
            400.00, 0.00, 60.00, '[]'::jsonb,
            460.00, 'flexible'::cancellation_policy, '{}'::jsonb, 50.00,
            100.00, 20.00, 0.1500, 1.000000, NOW(), NOW()
        )"#,
    )
    .bind(booking_hidden)
    .bind(guest_id)
    .bind(villa_unassigned)
    .bind(today - Duration::days(2))
    .bind(today)
    .execute(&pool)
    .await?;
    db_core::turnover::reconcile_for_booking(&pool, booking_hidden, "test_seed").await?;

    let app = app!(pool);

    // 1. Unauthenticated -> 401; Booker -> 403
    let unauth_resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/turnovers")
            .to_request(),
    )
    .await;
    assert_eq!(unauth_resp.status(), 401);

    let booker_resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/turnovers")
            .insert_header(bearer(booker))
            .to_request(),
    )
    .await;
    assert_eq!(booker_resp.status(), 403);

    // 2. Staff sees only assigned villa_a (never villa_unassigned or villa_b) and ZERO guest PII/financials
    let list_resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/turnovers?window=today")
            .insert_header(bearer(staff_a))
            .to_request(),
    )
    .await;
    assert_eq!(list_resp.status(), 200);
    let raw_list_bytes = test::read_body(list_resp).await;
    let raw_list = String::from_utf8_lossy(&raw_list_bytes);
    for forbidden in [
        guest_email,
        "SecretFirstName",
        "SecretLastName",
        conf_code,
        door_code,
        "1234.56",
        &guest_id.to_string(),
    ] {
        assert!(
            !raw_list.contains(forbidden),
            "PII/financial leak `{forbidden}` in turnover list JSON: {raw_list}"
        );
    }
    let staff_list: TurnoverListResponse = serde_json::from_slice(&raw_list_bytes)?;
    assert_eq!(staff_list.total, 1);
    let booking_turnover_id = staff_list.turnovers[0].id;
    assert_eq!(staff_list.turnovers[0].listing_id, villa_a);
    assert_eq!(
        staff_list.turnovers[0].source_kind,
        TurnoverSourceKind::Booking
    );

    let hidden_turnover_id: Uuid =
        sqlx::query_scalar("SELECT id FROM turnover WHERE listing_id = $1")
            .bind(villa_unassigned)
            .fetch_one(&pool)
            .await?;

    // Unassigned listing access is blocked (404) on GET detail, POST status, POST manual create.
    assert_eq!(
        test::call_service(
            &app,
            test::TestRequest::get()
                .uri(&format!("/api/v1/turnovers/{hidden_turnover_id}"))
                .insert_header(bearer(staff_a))
                .to_request(),
        )
        .await
        .status(),
        404
    );
    assert_eq!(
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/api/v1/turnovers/{hidden_turnover_id}/status"))
                .insert_header(bearer(staff_a))
                .set_json(serde_json::json!({ "to_status": "cleaning_in_progress" }))
                .to_request(),
        )
        .await
        .status(),
        404
    );
    assert_eq!(
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/v1/turnovers")
                .insert_header(bearer(staff_a))
                .set_json(serde_json::json!({
                    "listing_id": villa_b,
                    "checkout_date": today,
                    "title": "Blocked cross-host create"
                }))
                .to_request(),
        )
        .await
        .status(),
        403
    );

    // 3. Staff cannot set InspectedAndReady or mark booking turnover NotRequired -> 403 TURNOVER_TRANSITION_FORBIDDEN
    let forbid_inspect = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/turnovers/{booking_turnover_id}/status"))
            .insert_header(bearer(staff_a))
            .set_json(serde_json::json!({
                "to_status": "inspected_and_ready",
                "expected_status": "needs_cleaning"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(forbid_inspect.status(), 403);
    let forbid_body: serde_json::Value = test::read_body_json(forbid_inspect).await;
    assert_eq!(forbid_body["code"], "TURNOVER_TRANSITION_FORBIDDEN");

    let forbid_not_req = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/turnovers/{booking_turnover_id}/status"))
            .insert_header(bearer(staff_a))
            .set_json(serde_json::json!({ "to_status": "not_required" }))
            .to_request(),
    )
    .await;
    assert_eq!(forbid_not_req.status(), 403);

    // 4. Staff transitions NeedsCleaning -> CleaningCompleted; Host marks InspectedAndReady
    let staff_done_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/turnovers/{booking_turnover_id}/status"))
            .insert_header(bearer(staff_a))
            .set_json(serde_json::json!({
                "to_status": "cleaning_completed",
                "expected_status": "needs_cleaning",
                "reason": "Kitchen & baths finished"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(staff_done_resp.status(), 200);

    // Stale expected_status returns 409 TURNOVER_STATUS_CONFLICT
    let conflict_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/turnovers/{booking_turnover_id}/status"))
            .insert_header(bearer(host_a))
            .set_json(serde_json::json!({
                "to_status": "inspected_and_ready",
                "expected_status": "needs_cleaning"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(conflict_resp.status(), 409);
    let conflict_body: serde_json::Value = test::read_body_json(conflict_resp).await;
    assert_eq!(conflict_body["code"], "TURNOVER_STATUS_CONFLICT");

    let host_ready_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/turnovers/{booking_turnover_id}/status"))
            .insert_header(bearer(host_a))
            .set_json(serde_json::json!({
                "to_status": "inspected_and_ready",
                "expected_status": "cleaning_completed"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(host_ready_resp.status(), 200);

    // 5. Staff CAN unset InspectedAndReady -> MaintenanceRequired or NeedsCleaning
    let unset_maint_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/turnovers/{booking_turnover_id}/status"))
            .insert_header(bearer(staff_a))
            .set_json(serde_json::json!({
                "to_status": "maintenance_required",
                "expected_status": "inspected_and_ready",
                "reason": "Leaking AC unit discovered"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(unset_maint_resp.status(), 200);
    let unset_body: TurnoverResponse = test::read_body_json(unset_maint_resp).await;
    assert_eq!(unset_body.status, TurnoverStatus::MaintenanceRequired);

    // Re-inspect by Host, then Staff unsets InspectedAndReady -> NeedsCleaning
    assert_eq!(
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/api/v1/turnovers/{booking_turnover_id}/status"))
                .insert_header(bearer(host_a))
                .set_json(serde_json::json!({ "to_status": "inspected_and_ready" }))
                .to_request(),
        )
        .await
        .status(),
        200
    );
    let unset_clean_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/turnovers/{booking_turnover_id}/status"))
            .insert_header(bearer(staff_a))
            .set_json(serde_json::json!({
                "to_status": "needs_cleaning",
                "expected_status": "inspected_and_ready",
                "reason": "Guest dog footprints in foyer"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(unset_clean_resp.status(), 200);
    let unset_clean_body: TurnoverResponse = test::read_body_json(unset_clean_resp).await;
    assert_eq!(unset_clean_body.status, TurnoverStatus::NeedsCleaning);

    // 6. Staff creates manual turnover on assigned villa_a (201 Created), detail omits PII
    let manual_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/turnovers")
            .insert_header(bearer(staff_a))
            .set_json(serde_json::json!({
                "listing_id": villa_a,
                "checkout_date": today + Duration::days(1),
                "title": "Deep cistern scrub"
            }))
            .to_request(),
    )
    .await;
    assert_eq!(manual_resp.status(), 201);
    let manual: TurnoverResponse = test::read_body_json(manual_resp).await;
    assert_eq!(manual.source_kind, TurnoverSourceKind::Manual);

    // Detail endpoint includes history and hides PII
    let detail_resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/v1/turnovers/{booking_turnover_id}"))
            .insert_header(bearer(staff_a))
            .to_request(),
    )
    .await;
    assert_eq!(detail_resp.status(), 200);
    let detail_bytes = test::read_body(detail_resp).await;
    let detail_str = String::from_utf8_lossy(&detail_bytes);
    assert!(!detail_str.contains(guest_email));
    assert!(!detail_str.contains(conf_code));
    assert!(!detail_str.contains(door_code));
    let detail: TurnoverDetailResponse = serde_json::from_slice(&detail_bytes)?;
    assert!(detail.status_history.len() >= 4);

    // Admin sees turnovers across hosts
    let admin_resp = test::call_service(
        &app,
        test::TestRequest::get()
            .uri("/api/v1/turnovers?window=all")
            .insert_header(bearer(admin))
            .to_request(),
    )
    .await;
    assert_eq!(admin_resp.status(), 200);
    let admin_list: TurnoverListResponse = test::read_body_json(admin_resp).await;
    assert_eq!(admin_list.total, 3);

    Ok(())
}

#[tokio::test]
async fn turnover_comments_fanout_outbox_redact_logs_and_cron_republishes()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::settings::CronSettings;
    use api_core::email_publisher::EmailPublisher;
    use common::turnover::TurnoverCommentResponse;

    let test_db = test_db();
    let pool = test_db.get_pool().await;

    let host_a = user(&pool, UserRole::Host).await;
    let staff_a = user(&pool, UserRole::Staff).await;
    let staff_unassigned = user(&pool, UserRole::Staff).await;

    let host_email: String = sqlx::query_scalar("SELECT email FROM \"user\" WHERE id = $1")
        .bind(host_a)
        .fetch_one(&pool)
        .await?;
    let staff_email: String = sqlx::query_scalar("SELECT email FROM \"user\" WHERE id = $1")
        .bind(staff_a)
        .fetch_one(&pool)
        .await?;

    let villa = seed_listing(&pool, host_a, "Outbox Breeze").await?;
    link_staff(&pool, host_a, staff_a, villa).await?;
    sqlx::query("INSERT INTO host_staff (host_user_id, staff_user_id) VALUES ($1, $2)")
        .bind(host_a)
        .bind(staff_unassigned)
        .execute(&pool)
        .await?;

    let today = listing_today(Utc::now());
    let created = db_core::turnover::create_manual_turnover(
        &pool,
        db_core::turnover::TurnoverActor {
            user_id: host_a,
            role: common::turnover::TurnoverActorRole::Host,
        },
        &common::turnover::CreateManualTurnoverRequest {
            listing_id: villa,
            checkout_date: today,
            title: Some("Pool chemical balance".into()),
            initial_status: None,
        },
    )
    .await?;

    let cron_settings = CronSettings {
        cron_secret: "test-comment-cron-secret".to_string(),
    };
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .app_data(web::Data::new(settings()))
            .app_data(web::Data::new(tax_settings()))
            .app_data(web::Data::new(cron_settings))
            .app_data(web::Data::new(EmailPublisher::disabled()))
            .configure(crate::apis::configure_routes),
    )
    .await;

    // Unassigned staff blocked (404).
    assert_eq!(
        test::call_service(
            &app,
            test::TestRequest::post()
                .uri(&format!("/api/v1/turnovers/{}/comments", created.id))
                .insert_header(bearer(staff_unassigned))
                .set_json(serde_json::json!({ "body": "Should fail" }))
                .to_request(),
        )
        .await
        .status(),
        404
    );

    // Assigned staff posts comment; capture logs to verify zero comment body & zero email leaks.
    let secret_comment_text = "CONFIDENTIAL_GATE_NOTE_98765_DO_NOT_LOG";
    let (logs, guard) = super::promo_hold_tests::capture_logs();
    let post_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/v1/turnovers/{}/comments", created.id))
            .insert_header(bearer(staff_a))
            .set_json(serde_json::json!({ "body": secret_comment_text }))
            .to_request(),
    )
    .await;
    // Allow spawned task one yield to execute while capture_logs is active.
    tokio::task::yield_now().await;
    drop(guard);

    assert_eq!(post_resp.status(), 201);
    let comment: TurnoverCommentResponse = test::read_body_json(post_resp).await;
    assert_eq!(comment.body, secret_comment_text);

    let log_text = logs.text();
    assert!(
        !log_text.contains(secret_comment_text),
        "comment body must never appear in tracing logs: {log_text}"
    );
    assert!(
        !log_text.contains(&host_email) && !log_text.contains(&staff_email),
        "recipient/author email must never appear in tracing logs: {log_text}"
    );

    // Outbox has pending notification for host_a only.
    let (outbox_id, outbox_recipient): (Uuid, String) = sqlx::query_as(
        "SELECT id, recipient_email FROM email_outbox WHERE template_id = 'turnover_comment_notification' LIMIT 1",
    )
    .fetch_one(&pool)
    .await?;
    assert_eq!(outbox_recipient, host_email);

    // Simulate stranded outbox row > 5 min old and invoke cron run-sweeps to republish.
    sqlx::query("UPDATE email_outbox SET created_at = NOW() - INTERVAL '10 minutes' WHERE id = $1")
        .bind(outbox_id)
        .execute(&pool)
        .await?;

    let sweep_resp = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/v1/internal/cron/run-sweeps")
            .insert_header(("x-cron-secret", "test-comment-cron-secret"))
            .to_request(),
    )
    .await;
    assert_eq!(sweep_resp.status(), 200);
    let sweep_body: serde_json::Value = test::read_body_json(sweep_resp).await;
    assert!(
        sweep_body["stuck_emails_republished"]
            .as_u64()
            .is_some_and(|n| n >= 1),
        "expected cron run-sweeps to republish stuck turnover comment outbox email: {sweep_body}"
    );

    Ok(())
}
