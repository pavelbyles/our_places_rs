use crate::error::Result;
use crate::models::{Review, ReviewToken};
use chrono::{Duration, Utc};
use common::models::ListingRatingSummary;
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::instrument;
use uuid::Uuid;

/// Creates a new single-use review token for a completed booking
#[instrument(skip(conn))]
pub async fn create_review_token(
    conn: &mut sqlx::PgConnection,
    booking_id: Uuid,
) -> Result<ReviewToken> {
    // Generate a secure random URL-safe token (UUID v4 is sufficiently random and secure for this use case)
    let token = Uuid::new_v4().to_string();

    let booking = sqlx::query!(
        "SELECT guest_id, listing_id, date_to FROM booking WHERE id = $1",
        booking_id
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound))?;

    // valid immediately upon completion
    let valid_from = Utc::now();
    // expires strictly 15 days after checkout date at 23:59:59 UTC
    let expires_at = (booking.date_to + Duration::days(15))
        .and_hms_opt(23, 59, 59)
        .unwrap_or_default()
        .and_utc();

    let review_token = sqlx::query_as!(
        ReviewToken,
        r#"
        INSERT INTO review_token (id, token, booking_id, guest_id, listing_id, valid_from, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, token, booking_id, guest_id, listing_id, valid_from, expires_at, used_at, created_at
        "#,
        Uuid::now_v7(),
        token,
        booking_id,
        booking.guest_id,
        booking.listing_id,
        valid_from,
        expires_at
    )
    .fetch_one(&mut *conn)
    .await?;

    Ok(review_token)
}

/// Evaluates eligibility and retrieves or creates a single-use review token for a booking within 15 days post-stay
#[instrument(skip(pool))]
pub async fn get_or_create_booking_review_token(
    pool: &PgPool,
    booking_id: Uuid,
    guest_id: Uuid,
) -> Result<common::models::BookingReviewEligibility> {
    // 1. Fetch booking details
    let booking = sqlx::query!(
        r#"
        SELECT id, guest_id, listing_id, status as "status: crate::models::BookingStatus", date_to
        FROM booking
        WHERE id = $1
        "#,
        booking_id
    )
    .fetch_optional(pool)
    .await?
    .ok_or(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound))?;

    if booking.guest_id != guest_id {
        return Err(crate::error::DbError::ValidationError(
            "User is not authorized to review this booking".to_string(),
        ));
    }

    if booking.status == crate::models::BookingStatus::Cancelled {
        return Ok(common::models::BookingReviewEligibility {
            booking_id,
            is_eligible: false,
            token: None,
            has_reviewed: false,
            days_remaining: None,
            status_message: "Cancelled bookings are not eligible for review".to_string(),
        });
    }

    // 2. Check if a review already exists for this booking
    let existing_review = sqlx::query!("SELECT id FROM review WHERE booking_id = $1", booking_id)
        .fetch_optional(pool)
        .await?;

    if existing_review.is_some() {
        return Ok(common::models::BookingReviewEligibility {
            booking_id,
            is_eligible: false,
            token: None,
            has_reviewed: true,
            days_remaining: None,
            status_message: "You have already submitted a review for this stay".to_string(),
        });
    }

    // 3. Date checks: stay must be concluded and within 15 days of date_to
    let today = Utc::now().date_naive();
    let checkout_date = booking.date_to;
    let cutoff_date = checkout_date + Duration::days(15);

    if today < checkout_date {
        return Ok(common::models::BookingReviewEligibility {
            booking_id,
            is_eligible: false,
            token: None,
            has_reviewed: false,
            days_remaining: None,
            status_message: "Reviews can only be submitted after your stay has ended".to_string(),
        });
    }

    if today > cutoff_date {
        return Ok(common::models::BookingReviewEligibility {
            booking_id,
            is_eligible: false,
            token: None,
            has_reviewed: false,
            days_remaining: Some(0),
            status_message: "The 15-day review period for this stay has expired".to_string(),
        });
    }

    let days_remaining = (cutoff_date - today).num_days();

    // 4. Look for an existing, unused, non-expired token
    let existing_token = sqlx::query!(
        r#"
        SELECT token 
        FROM review_token 
        WHERE booking_id = $1 
          AND used_at IS NULL 
          AND expires_at > NOW()
        ORDER BY created_at DESC 
        LIMIT 1
        "#,
        booking_id
    )
    .fetch_optional(pool)
    .await?;

    if let Some(tok) = existing_token {
        return Ok(common::models::BookingReviewEligibility {
            booking_id,
            is_eligible: true,
            token: Some(tok.token),
            has_reviewed: false,
            days_remaining: Some(days_remaining),
            status_message: "Eligible for review".to_string(),
        });
    }

    // 5. If no active token exists, create a new one valid until strictly 15 days post-checkout
    let new_token = Uuid::new_v4().to_string();
    let valid_from = Utc::now() - Duration::hours(1);
    let expires_at = cutoff_date
        .and_hms_opt(23, 59, 59)
        .unwrap_or_default()
        .and_utc();

    sqlx::query!(
        r#"
        INSERT INTO review_token (id, token, booking_id, guest_id, listing_id, valid_from, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        Uuid::now_v7(),
        new_token,
        booking_id,
        booking.guest_id,
        booking.listing_id,
        valid_from,
        expires_at
    )
    .execute(pool)
    .await?;

    Ok(common::models::BookingReviewEligibility {
        booking_id,
        is_eligible: true,
        token: Some(new_token),
        has_reviewed: false,
        days_remaining: Some(days_remaining),
        status_message: "Eligible for review".to_string(),
    })
}

/// Retrieves a review token, checking its validity timeline
#[instrument(skip(pool))]
pub async fn get_token_info(pool: &PgPool, token_str: &str) -> Result<Option<ReviewToken>> {
    let token = sqlx::query_as!(
        ReviewToken,
        r#"
        SELECT id, token, booking_id, guest_id, listing_id, valid_from, expires_at, used_at, created_at
        FROM review_token
        WHERE token = $1
        "#,
        token_str
    )
    .fetch_optional(pool)
    .await?;

    Ok(token)
}

/// Transactionally consumes a token, inserts a review, and updates listing aggregate ratings
#[instrument(skip(pool, req))]
pub async fn create_review_with_token(
    pool: &PgPool,
    token_str: &str,
    req: &common::models::NewReviewRequest,
) -> Result<Review> {
    let mut tx = pool.begin().await?;

    // Consume the token atomically
    let token = sqlx::query_as!(
        ReviewToken,
        r#"
        UPDATE review_token
        SET used_at = NOW()
        WHERE token = $1 
          AND used_at IS NULL 
          AND valid_from <= NOW() 
          AND expires_at > NOW()
        RETURNING id, token, booking_id, guest_id, listing_id, valid_from, expires_at, used_at, created_at
        "#,
        token_str
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| crate::error::DbError::ValidationError("Token is invalid, expired, or has already been used".to_string()))?;

    // Acquire row-level lock on listing to serialize concurrent reviews
    let _ = sqlx::query!(
        "SELECT id FROM listing WHERE id = $1 FOR UPDATE",
        token.listing_id
    )
    .fetch_optional(&mut *tx)
    .await?;

    // Compute overall rating
    let overall_rating = req.calculate_overall_rating();

    // Insert the review
    let review = sqlx::query_as!(
        Review,
        r#"
        INSERT INTO review (
            id, booking_id, listing_id, guest_id, 
            cleanliness_rating, accuracy_rating, location_rating, value_rating, overall_rating, 
            public_review_text, private_host_feedback
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        RETURNING id, booking_id, listing_id, guest_id, 
                  cleanliness_rating, accuracy_rating, location_rating, value_rating, overall_rating, 
                  public_review_text, private_host_feedback, host_reply_text, host_replied_at, 
                  created_at, updated_at
        "#,
        Uuid::now_v7(),
        token.booking_id,
        token.listing_id,
        token.guest_id,
        req.cleanliness_rating,
        req.accuracy_rating,
        req.location_rating,
        req.value_rating,
        overall_rating,
        req.public_review_text,
        req.private_host_feedback
    )
    .fetch_one(&mut *tx)
    .await?;

    // Recalculate listing aggregates
    let aggregates = sqlx::query!(
        r#"
        SELECT 
            AVG(overall_rating) as "avg_overall: rust_decimal::Decimal", 
            COUNT(*) as "review_count!"
        FROM review
        WHERE listing_id = $1
        "#,
        token.listing_id
    )
    .fetch_one(&mut *tx)
    .await?;

    // Default to overall_rating if the aggregate query somehow returned NULL
    let new_avg = aggregates.avg_overall.unwrap_or(overall_rating);

    // Update listing table
    sqlx::query!(
        r#"
        UPDATE listing
        SET overall_rating = $1, review_count = $2
        WHERE id = $3
        "#,
        new_avg,
        aggregates.review_count as i32,
        token.listing_id
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(review)
}

/// Allows host to add an immutable reply to a review
#[instrument(skip(pool))]
pub async fn add_host_reply(
    pool: &PgPool,
    review_id: Uuid,
    host_id: Uuid,
    reply_text: &str,
) -> Result<Review> {
    let mut tx = pool.begin().await?;

    // Verify ownership and check that a reply doesn't exist yet
    let review = sqlx::query!(
        r#"
        SELECT r.id, r.host_reply_text, l.user_id as host_user_id
        FROM review r
        JOIN listing l ON r.listing_id = l.id
        WHERE r.id = $1
        FOR UPDATE OF r
        "#,
        review_id
    )
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound))?;

    if review.host_user_id != host_id {
        return Err(crate::error::DbError::ValidationError(
            "Only the listing owner can reply to this review".to_string(),
        ));
    }

    if review.host_reply_text.is_some() {
        return Err(crate::error::DbError::ValidationError(
            "A host reply has already been submitted for this review".to_string(),
        ));
    }

    let updated_review = sqlx::query_as!(
        Review,
        r#"
        UPDATE review
        SET host_reply_text = $1, host_replied_at = NOW(), updated_at = NOW()
        WHERE id = $2
        RETURNING id, booking_id, listing_id, guest_id, 
                  cleanliness_rating, accuracy_rating, location_rating, value_rating, overall_rating, 
                  public_review_text, private_host_feedback, host_reply_text, host_replied_at, 
                  created_at, updated_at
        "#,
        reply_text,
        review_id
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(updated_review)
}

/// Gets paginated public reviews for a listing
#[instrument(skip(pool))]
pub async fn get_listing_reviews(
    pool: &PgPool,
    listing_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<common::models::ReviewResponse>> {
    let reviews = sqlx::query_as!(
        common::models::ReviewResponse,
        r#"
        SELECT 
            r.id, u.first_name as guest_first_name, 
            r.cleanliness_rating, r.accuracy_rating, r.location_rating, r.value_rating, 
            r.overall_rating::FLOAT8 as "overall_rating!", 
            r.public_review_text, r.host_reply_text, r.host_replied_at, r.created_at
        FROM review r
        JOIN "user" u ON r.guest_id = u.id
        WHERE r.listing_id = $1 AND r.public_review_text IS NOT NULL
        ORDER BY r.created_at DESC
        LIMIT $2 OFFSET $3
        "#,
        listing_id,
        limit,
        offset
    )
    .fetch_all(pool)
    .await?;

    Ok(reviews)
}

/// Retrieves the aggregated rating summary for a listing
#[instrument(skip(conn))]
pub async fn get_listing_rating_summary(
    conn: &mut sqlx::PgConnection,
    listing_id: Uuid,
) -> Result<ListingRatingSummary> {
    let aggregates = sqlx::query!(
        r#"
        SELECT 
            AVG(overall_rating)::FLOAT8 as "overall",
            AVG(cleanliness_rating)::FLOAT8 as "cleanliness",
            AVG(accuracy_rating)::FLOAT8 as "accuracy",
            AVG(location_rating)::FLOAT8 as "location",
            AVG(value_rating)::FLOAT8 as "value",
            COUNT(*) as "count!"
        FROM review
        WHERE listing_id = $1
        "#,
        listing_id
    )
    .fetch_one(&mut *conn)
    .await?;

    let count = aggregates.count as i32;

    let distributions = sqlx::query!(
        r#"
        SELECT ROUND(overall_rating) as "stars!", COUNT(*) as "cnt!"
        FROM review
        WHERE listing_id = $1
        GROUP BY ROUND(overall_rating)
        "#,
        listing_id
    )
    .fetch_all(&mut *conn)
    .await?;

    use rust_decimal::prelude::ToPrimitive;
    let mut dist_map = HashMap::new();
    for d in distributions {
        dist_map.insert(d.stars.to_i32().unwrap_or(0), d.cnt as i32);
    }

    Ok(ListingRatingSummary {
        overall_rating: aggregates.overall,
        cleanliness_rating: aggregates.cleanliness,
        accuracy_rating: aggregates.accuracy,
        location_rating: aggregates.location,
        value_rating: aggregates.value,
        review_count: count,
        rating_distribution: dist_map,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    use sqlx_db_tester::TestPg;
    use std::env;
    use std::path::Path;

    async fn setup_test_db() -> TestPg {
        dotenvy::dotenv().ok();
        let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:postgres@localhost:5432/our_places".to_string()
        });
        TestPg::new(db_url, Path::new("migrations"))
    }

    #[tokio::test]
    async fn test_review_flow() {
        let test_db = setup_test_db().await;
        let pool = test_db.get_pool().await;
        let mut tx = pool.begin().await.expect("Failed to begin tx");

        // 1. Setup Data
        let host_id = Uuid::now_v7();
        let guest_id = Uuid::now_v7();
        let booking_id = Uuid::now_v7();

        // Create Host
        crate::user::create_user(
            &mut *tx,
            &crate::models::NewUser {
                id: host_id,
                email: "host@example.com".to_string(),
                password_hash: "hash".to_string(),
                first_name: "Host".to_string(),
                last_name: "User".to_string(),
                phone_number: None,
                is_active: true,
                is_verified: true,
                verification_code: None,
                verification_code_expires_at: None,
                attributes: serde_json::json!({}),
                roles: Some(vec![crate::models::UserRole::Host]),
                default_currency: "USD".to_string(),
            },
        )
        .await
        .unwrap();

        // Create Guest
        crate::user::create_user(
            &mut *tx,
            &crate::models::NewUser {
                id: guest_id,
                email: "guest@example.com".to_string(),
                password_hash: "hash".to_string(),
                first_name: "Guest".to_string(),
                last_name: "User".to_string(),
                phone_number: None,
                is_active: true,
                is_verified: true,
                verification_code: None,
                verification_code_expires_at: None,
                attributes: serde_json::json!({}),
                roles: Some(vec![crate::models::UserRole::Booker]),
                default_currency: "USD".to_string(),
            },
        )
        .await
        .unwrap();

        // Create Listing
        // Create Listing
        let listing_id = Uuid::now_v7();
        let _ = sqlx::query(
            "INSERT INTO listing (id, user_id, name, listing_structure_id, country, slug, max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, base_currency, minimum_stay, added_at) VALUES ($1, $2, $3, 1, 'Jamaica', 'test-villa', 2, 1, 1, 1, 0, 'USD', 1, NOW())",
        )
        .bind(listing_id)
        .bind(host_id)
        .bind("Test Villa")
        .execute(&mut *tx)
        .await
        .unwrap();

        // Create Booking (need to use raw SQL since create_booking sets status to pending and we need completed)
        let _ = sqlx::query(
            "INSERT INTO booking (id, confirmation_code, guest_id, listing_id, status, date_from, date_to, currency, daily_rate, number_of_persons, total_days, sub_total_price, total_price, cancellation_policy, metadata) VALUES ($1, 'ABCDEF', $2, $3, 'completed', CURRENT_DATE - INTERVAL '5 days', CURRENT_DATE, 'USD', 100, 2, 4, 400, 400, 'flexible', '{}')",
        )
        .bind(booking_id)
        .bind(guest_id)
        .bind(listing_id)
        .execute(&mut *tx)
        .await
        .unwrap();

        tx.commit().await.unwrap();

        // 2. Generate token (simulating completion)
        let mut conn = pool.acquire().await.unwrap();
        let token = create_review_token(&mut conn, booking_id).await.unwrap();
        assert_eq!(token.booking_id, booking_id);

        // Make the token valid immediately for the test
        sqlx::query(
            "UPDATE review_token SET valid_from = NOW() - INTERVAL '1 day' WHERE token = $1",
        )
        .bind(&token.token)
        .execute(&pool)
        .await
        .unwrap();

        // 3. Get token info
        let token_info = get_token_info(&pool, &token.token).await.unwrap();
        assert!(token_info.is_some());

        // 4. Submit review
        let review_req = common::models::NewReviewRequest {
            token: token.token.clone(),
            cleanliness_rating: 5,
            accuracy_rating: 4,
            location_rating: 5,
            value_rating: 4,
            public_review_text: Some("Great place!".to_string()),
            private_host_feedback: Some("Needs more towels.".to_string()),
        };

        let review = create_review_with_token(&pool, &token.token, &review_req)
            .await
            .unwrap();
        assert_eq!(review.overall_rating, dec!(4.50));

        // 5. Trying to reuse token should fail
        let reuse_err = create_review_with_token(&pool, &token.token, &review_req).await;
        assert!(reuse_err.is_err());

        // 6. Host replies
        let updated_review = add_host_reply(&pool, review.id, host_id, "Thanks for staying!")
            .await
            .unwrap();
        assert_eq!(
            updated_review.host_reply_text.unwrap(),
            "Thanks for staying!"
        );

        // 7. Verify aggregates
        let mut conn = pool.acquire().await.unwrap();
        let summary = get_listing_rating_summary(&mut *conn, listing_id)
            .await
            .unwrap();
        assert_eq!(summary.review_count, 1);
        assert_eq!(summary.overall_rating.unwrap(), 4.5);
    }
}
