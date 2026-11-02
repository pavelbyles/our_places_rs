//! Creating, updating and deleting listings.

use super::generate_listing_slug_native;
use crate::error::{DbError, Result};
use crate::models::{Listing, NewListing, UpdatedListing};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

/// Creates a new listing in the database.
#[tracing::instrument(skip(executor))]
pub async fn create_listing<'e, E>(executor: E, new_listing: &NewListing) -> Result<Listing>
where
    E: PgExecutor<'e>,
{
    let guests_included = new_listing.guests_included.unwrap_or(1);
    if guests_included < 1 || guests_included > new_listing.max_guests {
        return Err(crate::error::DbError::ValidationError(
            "Guests included must be between 1 and max_guests".to_string(),
        ));
    }

    let listing = sqlx::query_as!(
        Listing,
        r#"
        INSERT INTO listing (
            id, user_id, name, description, listing_structure_id, country, price_per_night, 
            weekly_discount_percentage, monthly_discount_percentage, added_at, slug, 
            max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, square_meters, 
            latitude, longitude, listing_details, overall_rating, review_count, city, base_currency, minimum_stay, days_between_bookings, commission_pct, cleaning_fee, damage_deposit, guests_included, extra_guest_fee
        )
        SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, COALESCE($20, '{}'::jsonb), $21, $22, $23, $24, $25, $26, COALESCE($27, 0.0000), COALESCE($28, 0.00), COALESCE($29, 0.00), COALESCE($30, 1), COALESCE($31, 0.00)
        WHERE EXISTS (SELECT 1 FROM host_profiles WHERE user_id = $2)
        RETURNING 
            id, user_id, name, description, listing_structure_id, country, price_per_night, 
            is_active, added_at, deleted_at, CAST(NULL AS TEXT) as primary_image_url, 
            weekly_discount_percentage, monthly_discount_percentage, slug, 
            max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, square_meters, 
            latitude, longitude, CAST(overall_rating AS FLOAT8) as overall_rating, review_count, listing_details, city, base_currency, minimum_stay, days_between_bookings, commission_pct, cleaning_fee, damage_deposit, guests_included, extra_guest_fee
        "#,
        Uuid::now_v7(),                                  // $1
        new_listing.user_id,                             // $2
        new_listing.name,                                // $3
        new_listing.description,                         // $4
        new_listing.listing_structure_id,                // $5
        new_listing.country,                             // $6
        new_listing.price_per_night,                     // $7
        new_listing.weekly_discount_percentage,          // $8
        new_listing.monthly_discount_percentage,         // $9
        chrono::Utc::now(),                              // $10 (Explicitly mapped added_at!)
        generate_listing_slug_native(&new_listing.name), // $11
        new_listing.max_guests,                          // $12
        new_listing.bedrooms,                            // $13
        new_listing.beds,                                // $14
        new_listing.full_bathrooms,                      // $15
        new_listing.half_bathrooms,                      // $16
        new_listing.square_meters,                       // $17
        new_listing.latitude,                            // $18
        new_listing.longitude,                           // $19
        new_listing.listing_details,                     // $20
        rust_decimal::Decimal::ZERO,                     // $21
        0i32,                                            // $22
        new_listing.city,                                // $23
        new_listing.base_currency,                       // $24
        new_listing.minimum_stay,                        // $25
        new_listing.days_between_bookings,               // $26
        new_listing.commission_pct,                      // $27
        new_listing.cleaning_fee,                        // $28
        new_listing.damage_deposit,                      // $29
        new_listing.guests_included,                     // $30
        new_listing.extra_guest_fee,                     // $31
    )
    .fetch_one(executor)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => crate::error::DbError::ValidationError(
            "User must have a host profile to create a listing".to_string(),
        ),
        other => crate::error::DbError::Sqlx(other),
    })?;

    Ok(listing)
}

/// Updates a listing in the database.
#[tracing::instrument(skip(pool))]
/// Creates a listing after checking, in the same transaction, that its base
/// currency exists and is active (`CURRENCY_NOT_FOUND` / `CURRENCY_INACTIVE`).
pub async fn create_listing_with_active_currency(
    pool: &PgPool,
    new_listing: &NewListing,
) -> Result<Listing> {
    let mut tx = pool.begin().await?;
    crate::currency::ensure_active(&mut tx, &new_listing.base_currency).await?;
    let listing = create_listing(&mut *tx, new_listing).await?;
    tx.commit().await?;
    Ok(listing)
}

pub async fn update_listing(
    pool: &PgPool,
    id: Uuid,
    updated_listing_data: &UpdatedListing,
) -> Result<Listing> {
    let mut tx = pool.begin().await?;
    let updated = update_listing_with_conn(&mut tx, id, updated_listing_data).await?;
    tx.commit().await?;
    Ok(updated)
}

pub async fn update_listing_with_conn(
    tx: &mut sqlx::PgConnection,
    id: Uuid,
    updated_listing_data: &UpdatedListing,
) -> Result<Listing> {
    let current = sqlx::query_as!(
        Listing,
        r#"SELECT id, user_id, name, description, listing_structure_id, country, price_per_night, is_active, added_at, deleted_at, CAST(NULL AS TEXT) as primary_image_url, weekly_discount_percentage, monthly_discount_percentage, slug, max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, square_meters, latitude, longitude, CAST(overall_rating AS FLOAT8) as overall_rating, review_count, listing_details, city, base_currency, minimum_stay, days_between_bookings, commission_pct, cleaning_fee, damage_deposit, guests_included, extra_guest_fee FROM listing WHERE id = $1 FOR UPDATE"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;

    let effective_max_guests = updated_listing_data
        .max_guests
        .unwrap_or(current.max_guests);
    let effective_guests_included = updated_listing_data
        .guests_included
        .unwrap_or(current.guests_included);
    if let Some(new_base) = updated_listing_data
        .base_currency
        .as_deref()
        .filter(|base| *base != current.base_currency)
    {
        // Same transaction as the update, after the listing row lock.
        crate::currency::ensure_active(tx, new_base).await?;
    }

    if effective_guests_included < 1 || effective_guests_included > effective_max_guests {
        return Err(DbError::ValidationError(
            "guests_included must be between 1 and max_guests".to_string(),
        ));
    }

    // 2. Archive current state to history via direct table copy to ensure 100% data fidelity
    sqlx::query!(
        r#"
        INSERT INTO listing_history (
            listing_id, user_id, name, description, listing_structure_id, country, 
            price_per_night, is_active, weekly_discount_percentage, monthly_discount_percentage, 
            slug, max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, square_meters, 
            latitude, longitude, overall_rating, review_count, listing_details, valid_from, minimum_stay, days_between_bookings, commission_pct, cleaning_fee, damage_deposit, guests_included, extra_guest_fee
        )
        SELECT 
            id, user_id, name, description, listing_structure_id, country, 
            price_per_night, is_active, weekly_discount_percentage, monthly_discount_percentage, 
            slug, max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, square_meters, 
            latitude, longitude, overall_rating, review_count, listing_details, added_at, minimum_stay, days_between_bookings, commission_pct, cleaning_fee, damage_deposit, guests_included, extra_guest_fee
        FROM listing 
        WHERE id = $1
        "#,
        id
    )
    .execute(&mut *tx)
    .await?;

    // 3. Update the record
    // We use COALESCE($2, name) to say: "If the new name is NULL, keep the old name".
    let updated = sqlx::query_as!(
        Listing,
        r#"
        UPDATE listing
        SET
            name = COALESCE($2, name),
            description = COALESCE($3, description),
            listing_structure_id = COALESCE($4, listing_structure_id),
            country = COALESCE($5, country),
            price_per_night = COALESCE($6, price_per_night),
            is_active = COALESCE($7, is_active),
            weekly_discount_percentage = COALESCE($8, weekly_discount_percentage),
            monthly_discount_percentage = COALESCE($9, monthly_discount_percentage),
            max_guests = COALESCE($10, max_guests),
            bedrooms = COALESCE($11, bedrooms),
            beds = COALESCE($12, beds),
            full_bathrooms = COALESCE($13, full_bathrooms),
            half_bathrooms = COALESCE($14, half_bathrooms),
            square_meters = COALESCE($15, square_meters),
            latitude = COALESCE($16, latitude),
            longitude = COALESCE($17, longitude),
            listing_details = COALESCE($18, listing_details),
            city = COALESCE($19, city),
            base_currency = COALESCE($20, base_currency),
            minimum_stay = COALESCE($21, minimum_stay),
            days_between_bookings = COALESCE($22, days_between_bookings),
            commission_pct = COALESCE($23, commission_pct),
            cleaning_fee = COALESCE($24, cleaning_fee),
            damage_deposit = COALESCE($25, damage_deposit),
            guests_included = COALESCE($26, guests_included),
            extra_guest_fee = COALESCE($27, extra_guest_fee)
        WHERE id = $1
        RETURNING id, user_id, name, description, listing_structure_id, country, price_per_night, is_active, added_at, deleted_at, CAST(NULL AS TEXT) as primary_image_url, weekly_discount_percentage, monthly_discount_percentage, slug, max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, square_meters, latitude, longitude, CAST(overall_rating AS FLOAT8) as overall_rating, review_count, listing_details, city, base_currency, minimum_stay, days_between_bookings, commission_pct, cleaning_fee, damage_deposit, guests_included, extra_guest_fee
        "#,
        id,
        updated_listing_data.name,
        updated_listing_data.description,
        updated_listing_data.listing_structure_id,
        updated_listing_data.country,
        updated_listing_data.price_per_night,
        updated_listing_data.is_active,
        updated_listing_data.weekly_discount_percentage,
        updated_listing_data.monthly_discount_percentage,
        updated_listing_data.max_guests,
        updated_listing_data.bedrooms,
        updated_listing_data.beds,
        updated_listing_data.full_bathrooms,
        updated_listing_data.half_bathrooms,
        updated_listing_data.square_meters,
        updated_listing_data.latitude,
        updated_listing_data.longitude,
        updated_listing_data.listing_details,
        updated_listing_data.city,
        updated_listing_data.base_currency,
        updated_listing_data.minimum_stay,
        updated_listing_data.days_between_bookings,
        updated_listing_data.commission_pct,
        updated_listing_data.cleaning_fee,
        updated_listing_data.damage_deposit,
        updated_listing_data.guests_included,
        updated_listing_data.extra_guest_fee,
    )
    .fetch_one(&mut *tx)
    .await?;

    Ok(updated)
}

/// Deletes a listing from the database.
/// If `hard_delete` is true, the listing is permanently removed.
/// If `hard_delete` is false, the listing is soft-deleted (deleted_at is set).
#[tracing::instrument(skip(pool))]
pub async fn delete_listing(pool: &PgPool, id: Uuid, hard_delete: bool) -> Result<()> {
    let result: sqlx::postgres::PgQueryResult = if hard_delete {
        sqlx::query!("DELETE FROM listing WHERE id = $1", id)
            .execute(pool)
            .await
            .map_err(|e| match e.as_database_error().and_then(|d| d.code()).as_deref() {
                Some(
                    crate::error::SQLSTATE_RESTRICT_VIOLATION
                    | crate::error::SQLSTATE_FOREIGN_KEY_VIOLATION,
                ) => crate::error::DbError::ValidationError(
                    "Cannot permanently delete a listing with linked bookings or promo codes. Please archive it instead."
                        .to_string(),
                ),
                _ => crate::error::DbError::Sqlx(e),
            })?
    } else {
        sqlx::query!("UPDATE listing SET deleted_at = now() WHERE id = $1", id)
            .execute(pool)
            .await?
    };

    if result.rows_affected() == 0 {
        return Err(crate::error::DbError::Sqlx(sqlx::Error::RowNotFound));
    }

    Ok(())
}
