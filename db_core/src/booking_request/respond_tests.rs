//! Accepting, declining and withdrawing requests (#105, plan T16).

use super::fixtures::{
    day, lapse_request, listing_in_mode, migrated_db, new_request, raw_user, request_history,
    request_status,
};
use super::{Actor, accept_request, create_request, decline_request, withdraw_request};
use crate::booking::check_availability;
use crate::error::DbError;
use crate::models::DbMessageSenderRole;
use chrono::{Duration, Utc};
use common::booking_request::{DeclineReason, RequestRefusal, RequestStatus};
use sqlx::PgPool;
use uuid::Uuid;

const OFFER_HOURS: i64 = 48;

fn host_actor(id: Uuid) -> Actor {
    Actor {
        id,
        role: DbMessageSenderRole::Host,
    }
}

fn refusal<T: std::fmt::Debug>(result: crate::error::Result<T>) -> RequestRefusal {
    match result {
        Err(DbError::RequestRefused(r)) => r,
        other => panic!("expected a request refusal, got {other:?}"),
    }
}

async fn open_request(pool: &PgPool) -> (Uuid, Uuid, Uuid, Uuid) {
    let (villa, host) = listing_in_mode(pool, "request", 0).await;
    let guest = raw_user(pool).await;
    let created = create_request(pool, &new_request(villa, guest, day(3, 1), day(3, 4)))
        .await
        .expect("request");
    (created.request.id, villa, host, guest)
}

#[tokio::test]
async fn request_respond_accepting_holds_the_dates_as_an_offer_without_a_booking() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (id, villa, host, _) = open_request(&pool).await;

    let accepted = accept_request(&pool, id, host_actor(host), OFFER_HOURS)
        .await
        .expect("accept");
    assert_eq!(accepted.status, RequestStatus::Accepted);
    assert!(accepted.first_response_at.is_some());
    let offer = accepted.expires_at - Utc::now();
    assert!(
        offer > Duration::hours(47) && offer <= Duration::hours(48),
        "{offer}"
    );

    let bookings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM booking WHERE listing_id = $1")
        .bind(villa)
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(bookings, 0, "no booking until checkout");
    assert!(
        !check_availability(&pool, villa, day(3, 1), day(3, 4))
            .await
            .expect("check")
    );
    assert_eq!(
        request_history(&pool, id).await.last().cloned(),
        Some((Some("open".to_string()), "accepted".to_string(), Some(host)))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn request_respond_decline_and_withdraw_racing_have_one_winner() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    for _ in 0..5 {
        let (id, _, host, guest) = open_request(&pool).await;
        let (d, w) = tokio::join!(
            decline_request(&pool, id, host_actor(host), DeclineReason::StayLength, None),
            withdraw_request(&pool, id, guest)
        );
        assert_eq!([d.is_ok(), w.is_ok()].iter().filter(|ok| **ok).count(), 1);
        let loser = if d.is_ok() { refusal(w) } else { refusal(d) };
        assert_eq!(loser, RequestRefusal::RequestNotOpen);
        assert_eq!(
            request_history(&pool, id).await.len(),
            2,
            "create + one move"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn request_respond_accept_and_withdraw_racing_are_serialised() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    for _ in 0..5 {
        let (id, _, host, guest) = open_request(&pool).await;
        let (a, w) = tokio::join!(
            accept_request(&pool, id, host_actor(host), OFFER_HOURS),
            withdraw_request(&pool, id, guest)
        );
        // Accept then withdraw both succeed; withdraw first leaves nothing to accept.
        assert!(
            w.is_ok(),
            "the guest can always withdraw a live request or offer"
        );
        if let Err(e) = &a {
            assert!(matches!(
                e,
                DbError::RequestRefused(RequestRefusal::RequestNotOpen)
            ));
        }
        assert_eq!(request_status(&pool, id).await, "withdrawn");
        let moves = request_history(&pool, id).await.len() - 1;
        assert_eq!(
            moves,
            if a.is_ok() { 2 } else { 1 },
            "every success is recorded"
        );
    }
}

#[tokio::test]
async fn request_respond_accepting_after_the_window_is_refused_even_before_a_sweep() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (id, _, host, _) = open_request(&pool).await;
    lapse_request(&pool, id).await;
    assert_eq!(
        refusal(accept_request(&pool, id, host_actor(host), OFFER_HOURS).await),
        RequestRefusal::RequestExpired
    );
    assert_eq!(request_status(&pool, id).await, "open");
}

#[tokio::test]
async fn request_respond_decline_works_from_every_live_state_and_frees_the_dates() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    for from in ["open", "accepted", "quoted"] {
        let (id, villa, host, _) = open_request(&pool).await;
        sqlx::query("UPDATE booking_request SET status = $1::booking_request_status WHERE id = $2")
            .bind(from)
            .bind(id)
            .execute(&pool)
            .await
            .expect("set status");
        let declined = decline_request(
            &pool,
            id,
            host_actor(host),
            DeclineReason::GroupSize,
            Some("  Max 8 guests.  "),
        )
        .await
        .unwrap_or_else(|e| panic!("{from}: {e:?}"));
        assert_eq!(declined.status, RequestStatus::Declined);
        assert_eq!(declined.decline_reason, Some(DeclineReason::GroupSize));
        assert_eq!(declined.decline_note.as_deref(), Some("Max 8 guests."));
        assert!(
            check_availability(&pool, villa, day(3, 1), day(3, 4))
                .await
                .expect("check")
        );
    }
}

#[tokio::test]
async fn request_respond_other_needs_a_note_and_terminal_requests_stay_put() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (id, _, host, guest) = open_request(&pool).await;
    assert_eq!(
        refusal(
            decline_request(&pool, id, host_actor(host), DeclineReason::Other, Some(" ")).await
        ),
        RequestRefusal::DeclineNoteRequired
    );
    withdraw_request(&pool, id, guest).await.expect("withdraw");
    assert_eq!(
        refusal(accept_request(&pool, id, host_actor(host), OFFER_HOURS).await),
        RequestRefusal::RequestNotOpen
    );
    assert_eq!(
        refusal(withdraw_request(&pool, id, guest).await),
        RequestRefusal::RequestNotOpen
    );
}

#[tokio::test]
async fn request_respond_only_the_guest_withdraws_and_inactive_listings_refuse_accepts() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (id, villa, host, _) = open_request(&pool).await;
    let stranger = raw_user(&pool).await;
    assert_eq!(
        refusal(withdraw_request(&pool, id, stranger).await),
        RequestRefusal::NotParty
    );

    sqlx::query("UPDATE listing SET is_active = FALSE WHERE id = $1")
        .bind(villa)
        .execute(&pool)
        .await
        .expect("deactivate");
    assert!(matches!(
        accept_request(&pool, id, host_actor(host), OFFER_HOURS).await,
        Err(DbError::ListingUnavailable)
    ));
    decline_request(
        &pool,
        id,
        host_actor(host),
        DeclineReason::DatesUnavailable,
        None,
    )
    .await
    .expect("decline still works");
}

#[tokio::test]
async fn request_respond_an_offer_is_capped_at_check_in() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, host) = listing_in_mode(&pool, "request", 0).await;
    let guest = raw_user(&pool).await;
    let check_in = common::availability::jamaica_today(Utc::now()) + Duration::days(2);
    let mut request = new_request(villa, guest, check_in, check_in + Duration::days(3));
    request.expires_at = Utc::now() + Duration::hours(1);
    let created = create_request(&pool, &request).await.expect("request");
    let accepted = accept_request(&pool, created.request.id, host_actor(host), 720)
        .await
        .expect("accept");
    assert_eq!(
        accepted.expires_at,
        common::availability::jamaica_midnight_utc(check_in)
    );
}

#[tokio::test]
async fn request_respond_accepting_over_a_new_block_is_refused() {
    use crate::calendar::fixtures::{BlockKind, insert_block};
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    for kind in [BlockKind::Imported, BlockKind::Manual] {
        let (id, villa, host, _) = open_request(&pool).await;
        insert_block(&pool, villa, kind, day(3, 2), day(3, 3)).await;
        let result = accept_request(&pool, id, host_actor(host), OFFER_HOURS).await;
        assert!(
            matches!(result, Err(DbError::DateUnavailable)),
            "{kind:?}: {result:?}"
        );
        assert_eq!(request_status(&pool, id).await, "open");
    }
}
