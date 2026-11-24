//! Converting offers into `pending` holds (#105, plan T18).

use super::fixtures::{
    day, lapse_request, listing_in_mode, migrated_db, new_hold, new_request, raw_user,
    request_history, request_status, snapshot,
};
use super::{Actor, OfferCheckout, accept_request, convert_offer_to_hold, create_request};
use crate::booking::create_booking;
use crate::error::DbError;
use crate::models::DbMessageSenderRole;
use common::booking_request::{RequestRefusal, SnapshotPromo, StayPriceSnapshot};
use common::promo::{PromoDiscountType, PromoScope, PromoTerms};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use sqlx::PgPool;
use uuid::Uuid;

const HOLD_MINUTES: i32 = 120;

fn checkout(request_id: Uuid, guest_id: Uuid, quote_id: Option<Uuid>) -> OfferCheckout {
    OfferCheckout {
        request_id,
        guest_id,
        quote_id,
        confirmation_code: format!("C{}", Uuid::new_v4().simple())[..10].to_uppercase(),
        hold_minutes: HOLD_MINUTES,
    }
}

fn refusal<T: std::fmt::Debug>(result: crate::error::Result<T>) -> RequestRefusal {
    match result {
        Err(DbError::RequestRefused(r)) => r,
        other => panic!("expected a request refusal, got {other:?}"),
    }
}

struct Offer {
    id: Uuid,
    villa: Uuid,
    guest: Uuid,
}

/// An accepted request carrying `snapshot`.
async fn accepted(pool: &PgPool, snapshot: StayPriceSnapshot) -> Offer {
    let (villa, host) = listing_in_mode(pool, "request", 0).await;
    let guest = raw_user(pool).await;
    let mut request = new_request(villa, guest, day(3, 1), day(3, 4));
    request.snapshot = snapshot;
    let created = create_request(pool, &request).await.expect("request");
    accept_request(
        pool,
        created.request.id,
        Actor {
            id: host,
            role: DbMessageSenderRole::Host,
        },
        48,
    )
    .await
    .expect("accept");
    Offer {
        id: created.request.id,
        villa,
        guest,
    }
}

/// Makes `offer` quoted at `total`, returning the quote id.
async fn quote(pool: &PgPool, offer: &Offer, total: Decimal) -> Uuid {
    let host: Uuid = sqlx::query_scalar("SELECT user_id FROM listing WHERE id = $1")
        .bind(offer.villa)
        .fetch_one(pool)
        .await
        .expect("host");
    let snapshot = StayPriceSnapshot {
        total_price: total,
        promo: None,
        ..snapshot()
    };
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO booking_request_quote (request_id, stay_price_base, terms, price_snapshot,
             valid_until, created_by)
         VALUES ($1, 500, 'Wedding allowed', $2, NOW() + INTERVAL '1 day', $3) RETURNING id",
    )
    .bind(offer.id)
    .bind(sqlx::types::Json(&snapshot))
    .bind(host)
    .fetch_one(pool)
    .await
    .expect("quote");
    sqlx::query(
        "UPDATE booking_request SET status = 'quoted', current_quote_id = $2 WHERE id = $1",
    )
    .bind(offer.id)
    .bind(id)
    .execute(pool)
    .await
    .expect("mark quoted");
    id
}

async fn bookings_on(pool: &PgPool, villa: Uuid) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM booking WHERE listing_id = $1")
        .bind(villa)
        .fetch_one(pool)
        .await
        .expect("count")
}

#[tokio::test]
async fn request_convert_an_accepted_offer_becomes_a_hold_at_the_snapshot_price() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let offer = accepted(&pool, snapshot()).await;
    // A later price change must not reprice what the guest accepted.
    sqlx::query("UPDATE listing SET price_per_night = 999 WHERE id = $1")
        .bind(offer.villa)
        .execute(&pool)
        .await
        .expect("reprice listing");

    let converted = convert_offer_to_hold(&pool, &checkout(offer.id, offer.guest, None))
        .await
        .expect("convert");
    let b = &converted.booking;
    let s = snapshot();
    assert_eq!(b.status, crate::models::BookingStatus::Pending);
    assert_eq!(
        (
            b.total_price,
            b.sub_total_price,
            b.platform_fee,
            b.tax_value,
            b.daily_rate
        ),
        (
            s.total_price,
            s.sub_total_price,
            s.platform_fee,
            s.tax_value,
            s.daily_rate
        )
    );
    assert_eq!((b.date_from, b.date_to), (day(3, 1), day(3, 4)));
    assert_eq!(converted.request.booking_id, Some(b.id));
    assert_eq!(request_status(&pool, offer.id).await, "converted");
    assert_eq!(
        request_history(&pool, offer.id).await.last().cloned(),
        Some((
            Some("accepted".to_string()),
            "converted".to_string(),
            Some(offer.guest)
        ))
    );
    let minutes: i32 = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM expires_at - created_at)::INT / 60 FROM booking WHERE id = $1",
    )
    .bind(b.id)
    .fetch_one(&pool)
    .await
    .expect("hold window");
    assert_eq!(minutes, HOLD_MINUTES);
}

#[tokio::test]
async fn request_convert_a_quoted_offer_uses_the_quote_the_guest_saw() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let offer = accepted(&pool, snapshot()).await;
    let first = quote(&pool, &offer, dec!(999.00)).await;
    let current = quote(&pool, &offer, dec!(888.00)).await;

    assert_eq!(
        refusal(convert_offer_to_hold(&pool, &checkout(offer.id, offer.guest, Some(first))).await),
        RequestRefusal::QuoteSuperseded
    );
    assert_eq!(
        refusal(convert_offer_to_hold(&pool, &checkout(offer.id, offer.guest, None)).await),
        RequestRefusal::QuoteSuperseded
    );
    let converted = convert_offer_to_hold(&pool, &checkout(offer.id, offer.guest, Some(current)))
        .await
        .expect("convert the current quote");
    assert_eq!(converted.booking.total_price, dec!(888.00));
    assert!(converted.booking.promo_code_id.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn request_convert_has_no_release_gap_against_a_rival_hold() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    for _ in 0..5 {
        let offer = accepted(&pool, snapshot()).await;
        // Instant holds must be allowed, so only the dates can stop the rival.
        sqlx::query("UPDATE listing SET booking_mode = 'both' WHERE id = $1")
            .bind(offer.villa)
            .execute(&pool)
            .await
            .expect("allow instant holds");
        let rival = raw_user(&pool).await;
        let rival_hold = new_hold(offer.villa, rival, day(3, 1), day(3, 4));
        let mine = checkout(offer.id, offer.guest, None);
        let (c, r) = tokio::join!(
            convert_offer_to_hold(&pool, &mine),
            create_booking(&pool, &rival_hold, HOLD_MINUTES)
        );
        assert!(c.is_ok(), "{:?}", c.err());
        assert!(matches!(r, Err(DbError::DateUnavailable)), "{:?}", r.err());
        assert_eq!(bookings_on(&pool, offer.villa).await, 1);
    }
}

#[tokio::test]
async fn request_convert_expired_stale_and_unaccepted_offers_create_no_hold() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;

    let offer = accepted(&pool, snapshot()).await;
    lapse_request(&pool, offer.id).await;
    assert_eq!(
        refusal(convert_offer_to_hold(&pool, &checkout(offer.id, offer.guest, None)).await),
        RequestRefusal::RequestExpired
    );
    assert_eq!(bookings_on(&pool, offer.villa).await, 0);

    let offer = accepted(&pool, snapshot()).await;
    let q = quote(&pool, &offer, dec!(500)).await;
    lapse_request(&pool, offer.id).await;
    assert_eq!(
        refusal(convert_offer_to_hold(&pool, &checkout(offer.id, offer.guest, Some(q))).await),
        RequestRefusal::QuoteExpired
    );
    assert_eq!(bookings_on(&pool, offer.villa).await, 0);

    let (villa, _) = listing_in_mode(&pool, "request", 0).await;
    let guest = raw_user(&pool).await;
    let open = create_request(&pool, &new_request(villa, guest, day(3, 1), day(3, 4)))
        .await
        .expect("open request");
    assert_eq!(
        refusal(convert_offer_to_hold(&pool, &checkout(open.request.id, guest, None)).await),
        RequestRefusal::RequestNotOpen
    );
    let offer = accepted(&pool, snapshot()).await;
    let stranger = raw_user(&pool).await;
    assert_eq!(
        refusal(convert_offer_to_hold(&pool, &checkout(offer.id, stranger, None)).await),
        RequestRefusal::NotParty
    );
}

#[tokio::test]
async fn request_convert_redeems_the_reserved_promo_even_after_deactivation() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let admin = raw_user(&pool).await;
    let code = crate::promo_tests::insert_code(&pool, admin, "RESERVED-1").await;
    sqlx::query("UPDATE promo_code SET total_usage_limit = 1 WHERE id = $1")
        .bind(code)
        .execute(&pool)
        .await
        .expect("limit");
    let promo_snapshot = StayPriceSnapshot {
        promo: Some(SnapshotPromo {
            promo_code_id: code,
            terms: PromoTerms {
                code: "RESERVED-1".to_string(),
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
            stay_discount: dec!(30.00),
            cleaning_discount: dec!(0),
        }),
        ..snapshot()
    };
    let offer = accepted(&pool, promo_snapshot).await;
    sqlx::query("UPDATE promo_code SET deactivated_at = NOW() WHERE id = $1")
        .bind(code)
        .execute(&pool)
        .await
        .expect("deactivate");

    let converted = convert_offer_to_hold(&pool, &checkout(offer.id, offer.guest, None))
        .await
        .expect("the reserved use converts");
    assert_eq!(converted.booking.promo_code_id, Some(code));
    assert_eq!(converted.booking.promo_stay_discount, dec!(30.00));
    let redemptions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM promo_code_redemption WHERE promo_code_id = $1 AND booking_id = $2",
    )
    .bind(code)
    .bind(converted.booking.id)
    .fetch_one(&pool)
    .await
    .expect("redemptions");
    assert_eq!(redemptions, 1);
}

#[tokio::test]
async fn request_convert_an_inactive_currency_or_listing_is_refused() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let eur = StayPriceSnapshot {
        currency: "EUR".to_string(),
        ..snapshot()
    };
    let offer = accepted(&pool, eur).await;
    sqlx::query("UPDATE currencies SET active = FALSE WHERE code = 'EUR'")
        .execute(&pool)
        .await
        .expect("deactivate EUR");
    assert!(matches!(
        convert_offer_to_hold(&pool, &checkout(offer.id, offer.guest, None)).await,
        Err(DbError::CurrencyInactive(_))
    ));
    sqlx::query("UPDATE currencies SET active = TRUE WHERE code = 'EUR'")
        .execute(&pool)
        .await
        .expect("reactivate EUR");

    let offer = accepted(&pool, snapshot()).await;
    sqlx::query("UPDATE listing SET is_active = FALSE WHERE id = $1")
        .bind(offer.villa)
        .execute(&pool)
        .await
        .expect("deactivate listing");
    assert!(matches!(
        convert_offer_to_hold(&pool, &checkout(offer.id, offer.guest, None)).await,
        Err(DbError::ListingUnavailable)
    ));
}

#[tokio::test]
async fn request_convert_an_unpaid_hold_expires_normally_and_the_request_stays_converted() {
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let offer = accepted(&pool, snapshot()).await;
    let converted = convert_offer_to_hold(&pool, &checkout(offer.id, offer.guest, None))
        .await
        .expect("convert");
    sqlx::query("UPDATE booking SET expires_at = NOW() - INTERVAL '1 minute' WHERE id = $1")
        .bind(converted.booking.id)
        .execute(&pool)
        .await
        .expect("lapse hold");
    crate::booking::expire_stale_holds(&pool)
        .await
        .expect("sweep");
    assert_eq!(request_status(&pool, offer.id).await, "converted");
    assert!(
        crate::booking::check_availability(&pool, offer.villa, day(3, 1), day(3, 4))
            .await
            .expect("check")
    );
}

#[tokio::test]
async fn request_convert_an_offer_over_a_new_block_creates_no_hold() {
    use crate::calendar::fixtures::{BlockKind, insert_block};
    let test_db = migrated_db();
    let pool = test_db.get_pool().await;
    let offer = accepted(&pool, snapshot()).await;
    insert_block(
        &pool,
        offer.villa,
        BlockKind::Imported,
        day(3, 1),
        day(3, 2),
    )
    .await;
    let result = convert_offer_to_hold(&pool, &checkout(offer.id, offer.guest, None)).await;
    assert!(
        matches!(result, Err(DbError::DateUnavailable)),
        "{:?}",
        result.err()
    );
    assert_eq!(bookings_on(&pool, offer.villa).await, 0);
}
