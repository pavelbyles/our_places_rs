//! A live request with a promo code reserves a use of it (#105 D2, plan T13).

use super::create_request;
use super::fixtures::*;
use crate::booking::create_booking;
use crate::error::DbError;
use crate::models::NewBookingPromo;
use crate::promo_tests::insert_code;
use common::booking_request::SnapshotPromo;
use common::promo::{PromoDiscountType, PromoRefusal, PromoScope, PromoTerms};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx::PgPool;
use uuid::Uuid;

const HOLD_MINUTES: i32 = 120;

fn terms(code: &str) -> PromoTerms {
    PromoTerms {
        code: code.to_string(),
        discount_type: PromoDiscountType::Percentage,
        discount_value: dec!(10),
        scope: PromoScope::Stay,
        currency: None,
        exchange_rate: Decimal::ONE,
        min_nights: None,
        min_booking_value: None,
        stay_from: None,
        stay_to: None,
    }
}

async fn limited_code(pool: &PgPool, total: Option<i32>, per_guest: Option<i32>) -> Uuid {
    let admin = raw_user(pool).await;
    let code = format!("REQ-{}", &Uuid::new_v4().simple().to_string()[..8]).to_uppercase();
    let id = insert_code(pool, admin, &code).await;
    sqlx::query("UPDATE promo_code SET total_usage_limit = $1, per_guest_limit = $2 WHERE id = $3")
        .bind(total)
        .bind(per_guest)
        .bind(id)
        .execute(pool)
        .await
        .expect("set limits");
    id
}

fn request_with(
    code: Uuid,
    listing: Uuid,
    guest: Uuid,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> super::NewRequest {
    let mut request = new_request(listing, guest, from, to);
    request.snapshot.promo = Some(SnapshotPromo {
        promo_code_id: code,
        terms: terms("CODE"),
        stay_discount: dec!(30.00),
        cleaning_discount: dec!(0),
    });
    request
}

fn hold_with(
    code: Uuid,
    listing: Uuid,
    guest: Uuid,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
) -> crate::models::NewBooking {
    let mut hold = new_hold(listing, guest, from, to);
    hold.promo = Some(NewBookingPromo {
        promo_code_id: code,
        terms: terms("CODE"),
        stay_discount: dec!(30.00),
        cleaning_discount: dec!(0),
    });
    hold
}

fn usage_refusal<T: std::fmt::Debug>(result: crate::error::Result<T>) -> Option<PromoRefusal> {
    match result {
        Err(DbError::PromoRefused(r)) => Some(r),
        other => panic!("expected a promo refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn promo_request_usage_a_live_request_uses_up_a_limit_of_one() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let code = limited_code(&pool, Some(1), None).await;
    let (villa, _) = listing_in_mode(&pool, "both", 0).await;
    let (a, b) = (raw_user(&pool).await, raw_user(&pool).await);

    let held = create_request(&pool, &request_with(code, villa, a, day(3, 1), day(3, 4)))
        .await
        .expect("request with the code");
    assert_eq!(held.request.promo_code_id, Some(code));

    let other = create_booking(
        &pool,
        &hold_with(code, villa, b, day(5, 1), day(5, 4)),
        HOLD_MINUTES,
    )
    .await;
    assert_eq!(usage_refusal(other), Some(PromoRefusal::UsageLimitReached));
    let other = create_request(&pool, &request_with(code, villa, b, day(6, 1), day(6, 4))).await;
    assert_eq!(usage_refusal(other), Some(PromoRefusal::UsageLimitReached));

    // Lapsing frees the use straight away, with no sweep.
    lapse_request(&pool, held.request.id).await;
    create_booking(
        &pool,
        &hold_with(code, villa, b, day(5, 1), day(5, 4)),
        HOLD_MINUTES,
    )
    .await
    .expect("the use is free again");
}

#[tokio::test]
async fn promo_request_usage_withdrawn_or_quoted_requests_free_their_use() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    for status in ["withdrawn", "quoted"] {
        let code = limited_code(&pool, Some(1), None).await;
        let (villa, _) = listing_in_mode(&pool, "both", 0).await;
        let (a, b) = (raw_user(&pool).await, raw_user(&pool).await);
        let held = create_request(&pool, &request_with(code, villa, a, day(3, 1), day(3, 4)))
            .await
            .expect("request");
        sqlx::query("UPDATE booking_request SET status = $1::booking_request_status WHERE id = $2")
            .bind(status)
            .bind(held.request.id)
            .execute(&pool)
            .await
            .expect("set status");
        create_booking(
            &pool,
            &hold_with(code, villa, b, day(5, 1), day(5, 4)),
            HOLD_MINUTES,
        )
        .await
        .unwrap_or_else(|e| panic!("{status}: {e:?}"));
    }
}

#[tokio::test]
async fn promo_request_usage_the_per_guest_limit_counts_requests() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let code = limited_code(&pool, None, Some(1)).await;
    let (one, _) = listing_in_mode(&pool, "both", 0).await;
    let (two, _) = listing_in_mode(&pool, "both", 0).await;
    let guest = raw_user(&pool).await;

    create_request(&pool, &request_with(code, one, guest, day(3, 1), day(3, 4)))
        .await
        .expect("first request");
    let second = create_request(&pool, &request_with(code, two, guest, day(3, 1), day(3, 4))).await;
    assert_eq!(usage_refusal(second), Some(PromoRefusal::GuestLimitReached));

    // Another guest is unaffected by this guest's limit.
    let other = raw_user(&pool).await;
    create_request(&pool, &request_with(code, two, other, day(3, 1), day(3, 4)))
        .await
        .expect("another guest");
}

#[tokio::test]
async fn promo_request_usage_a_guests_own_request_gives_way_to_their_hold_on_the_listing() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let code = limited_code(&pool, Some(1), Some(1)).await;
    let (villa, _) = listing_in_mode(&pool, "both", 0).await;
    let guest = raw_user(&pool).await;
    create_request(
        &pool,
        &request_with(code, villa, guest, day(3, 1), day(3, 4)),
    )
    .await
    .expect("request");
    // The hold supersedes the request first, so its reserved use is free.
    create_booking(
        &pool,
        &hold_with(code, villa, guest, day(3, 1), day(3, 4)),
        HOLD_MINUTES,
    )
    .await
    .expect("hold on the same listing");
}

#[tokio::test]
async fn promo_request_usage_a_deactivated_code_is_refused_at_submission() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let code = limited_code(&pool, None, None).await;
    sqlx::query("UPDATE promo_code SET deactivated_at = NOW() WHERE id = $1")
        .bind(code)
        .execute(&pool)
        .await
        .expect("deactivate");
    let (villa, _) = listing_in_mode(&pool, "request", 0).await;
    let guest = raw_user(&pool).await;
    let result = create_request(
        &pool,
        &request_with(code, villa, guest, day(3, 1), day(3, 4)),
    )
    .await;
    assert_eq!(usage_refusal(result), Some(PromoRefusal::Deactivated));
}
