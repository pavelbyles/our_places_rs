//! Custom quotes (#105, plan T24).

use super::fixtures::*;
use super::{
    Actor, NewEnquiry, NewQuote, create_enquiry, create_request, quote_enquiry, send_quote,
};
use crate::booking::check_availability;
use crate::error::DbError;
use crate::models::DbMessageSenderRole;
use common::booking_request::{
    RequestOrigin, RequestRefusal, RequestStatus, SnapshotPromo, StayPriceSnapshot,
};
use common::promo::{PromoDiscountType, PromoScope, PromoTerms};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx::PgPool;
use uuid::Uuid;

fn host(id: Uuid) -> Actor {
    Actor {
        id,
        role: DbMessageSenderRole::Host,
    }
}

fn quoted(total: Decimal) -> NewQuote<'static> {
    NewQuote {
        stay_price_base: dec!(500.00),
        terms: "  Wedding allowed until 10pm.  ",
        snapshot: StayPriceSnapshot {
            total_price: total,
            promo: None,
            ..snapshot()
        },
        valid_hours: 48,
    }
}

fn refusal<T: std::fmt::Debug>(result: crate::error::Result<T>) -> RequestRefusal {
    match result {
        Err(DbError::RequestRefused(r)) => r,
        other => panic!("expected a request refusal, got {other:?}"),
    }
}

async fn enquiry(pool: &PgPool, villa: Uuid, guest: Uuid) -> Uuid {
    create_enquiry(
        pool,
        &NewEnquiry {
            listing_id: villa,
            guest_id: guest,
            sender_name: "Sam",
            date_from: day(3, 1),
            date_to: day(3, 4),
            number_of_persons: 6,
            currency: "USD",
            message: "Wedding?",
        },
    )
    .await
    .expect("enquiry")
    .enquiry
    .id
}

#[tokio::test]
async fn request_quote_tests_a_newer_quote_replaces_the_first() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, owner) = listing_in_mode(&pool, "request", 0).await;
    let guest = raw_user(&pool).await;
    let created = create_request(&pool, &new_request(villa, guest, day(3, 1), day(3, 4)))
        .await
        .expect("request");

    let first = send_quote(&pool, created.request.id, host(owner), &quoted(dec!(900)))
        .await
        .expect("first quote");
    assert_eq!(first.request.status, RequestStatus::Quoted);
    assert_eq!(first.quote.terms, "Wedding allowed until 10pm.");
    let second = send_quote(&pool, created.request.id, host(owner), &quoted(dec!(850)))
        .await
        .expect("second quote");
    assert_eq!(second.request.current_quote_id, Some(second.quote.id));

    let superseded: Vec<(Uuid, bool)> = sqlx::query_as(
        "SELECT id, superseded_at IS NOT NULL FROM booking_request_quote
         WHERE request_id = $1 ORDER BY created_at",
    )
    .bind(created.request.id)
    .fetch_all(&pool)
    .await
    .expect("quotes");
    assert_eq!(
        superseded,
        vec![(first.quote.id, true), (second.quote.id, false)]
    );
    let history = request_history(&pool, created.request.id).await;
    assert_eq!(
        history
            .iter()
            .map(|(f, t, _)| (f.as_deref(), t.as_str()))
            .collect::<Vec<_>>(),
        [
            (None, "open"),
            (Some("open"), "quoted"),
            (Some("quoted"), "quoted")
        ]
    );
}

#[tokio::test]
async fn request_quote_tests_a_quote_frees_the_reserved_promo_use() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let admin = raw_user(&pool).await;
    let code = crate::promo_tests::insert_code(&pool, admin, "QUOTE-FREES").await;
    sqlx::query("UPDATE promo_code SET total_usage_limit = 1 WHERE id = $1")
        .bind(code)
        .execute(&pool)
        .await
        .expect("limit");
    let (villa, owner) = listing_in_mode(&pool, "both", 0).await;
    let guest = raw_user(&pool).await;
    let mut request = new_request(villa, guest, day(3, 1), day(3, 4));
    request.snapshot.promo = Some(SnapshotPromo {
        promo_code_id: code,
        terms: PromoTerms {
            code: "QUOTE-FREES".to_string(),
            discount_type: PromoDiscountType::Percentage,
            discount_value: dec!(10),
            scope: PromoScope::Stay,
            currency: None,
            exchange_rate: Decimal::ONE,
            min_nights: None,
            min_booking_value: None,
            stay_from: None,
            stay_to: None,
        },
        stay_discount: dec!(30),
        cleaning_discount: dec!(0),
    });
    let created = create_request(&pool, &request).await.expect("request");
    let count = |pool: PgPool| async move {
        let mut conn = pool.acquire().await.expect("conn");
        let mut tx = sqlx::Connection::begin(&mut *conn).await.expect("tx");
        let (_, usage) = crate::promo::lock_and_count(&mut tx, code, Uuid::nil())
            .await
            .expect("count");
        usage
    };
    assert!(common::promo::check_usage(count(pool.clone()).await).is_err());
    send_quote(&pool, created.request.id, host(owner), &quoted(dec!(900)))
        .await
        .expect("quote");
    assert!(common::promo::check_usage(count(pool.clone()).await).is_ok());
}

#[tokio::test]
async fn request_quote_tests_quoting_an_enquiry_holds_its_dates_and_closes_it() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, owner) = listing_in_mode(&pool, "instant", 0).await;
    let guest = raw_user(&pool).await;
    let id = enquiry(&pool, villa, guest).await;

    let sent = quote_enquiry(&pool, id, host(owner), &quoted(dec!(1200)))
        .await
        .expect("quote on enquiry");
    assert_eq!(sent.request.origin, RequestOrigin::EnquiryQuote);
    assert_eq!(sent.request.status, RequestStatus::Quoted);
    assert_eq!(sent.request.enquiry_id, Some(id));
    assert!(sent.request.first_response_at.is_some());
    assert!(
        !check_availability(&pool, villa, day(3, 1), day(3, 4))
            .await
            .expect("check")
    );
    assert_eq!(
        request_history(&pool, sent.request.id).await,
        vec![(None, "quoted".to_string(), Some(owner))]
    );
    let reason: String =
        sqlx::query_scalar("SELECT closed_reason::text FROM booking_enquiry WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("closed");
    assert_eq!(reason, "quoted");
    assert_eq!(
        refusal(quote_enquiry(&pool, id, host(owner), &quoted(dec!(1))).await),
        RequestRefusal::RequestNotOpen
    );
}

#[tokio::test]
async fn request_quote_tests_an_enquiry_on_taken_dates_or_with_a_live_request_is_refused() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, owner) = listing_in_mode(&pool, "both", 0).await;
    let (guest, other) = (raw_user(&pool).await, raw_user(&pool).await);
    let id = enquiry(&pool, villa, guest).await;
    raw_stay(&pool, other, villa, day(3, 2), day(3, 3), "confirmed", 0).await;
    assert!(matches!(
        quote_enquiry(&pool, id, host(owner), &quoted(dec!(900))).await,
        Err(DbError::DateUnavailable)
    ));
    let status: String =
        sqlx::query_scalar("SELECT status::text FROM booking_enquiry WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await
            .expect("status");
    assert_eq!(status, "open", "a refused quote leaves the enquiry open");

    let (villa, owner) = listing_in_mode(&pool, "both", 0).await;
    let id = enquiry(&pool, villa, guest).await;
    create_request(&pool, &new_request(villa, guest, day(5, 1), day(5, 4)))
        .await
        .expect("live request");
    assert_eq!(
        refusal(quote_enquiry(&pool, id, host(owner), &quoted(dec!(900))).await),
        RequestRefusal::RequestAlreadyLive
    );
}

#[tokio::test]
async fn request_quote_tests_expired_or_accepted_requests_cannot_be_quoted() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let (villa, owner) = listing_in_mode(&pool, "request", 0).await;
    let guest = raw_user(&pool).await;
    let created = create_request(&pool, &new_request(villa, guest, day(3, 1), day(3, 4)))
        .await
        .expect("request");
    lapse_request(&pool, created.request.id).await;
    assert_eq!(
        refusal(send_quote(&pool, created.request.id, host(owner), &quoted(dec!(9))).await),
        RequestRefusal::RequestExpired
    );
}
