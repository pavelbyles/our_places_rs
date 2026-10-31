//! Date-ranged price overrides for a listing.

use super::*;

/// Creates a new price override for a listing.
#[tracing::instrument(skip(executor))]
pub async fn create_price_override<'e, E>(
    executor: E,
    listing_id: Uuid,
    req: &common::models::CreatePriceOverrideRequest,
) -> Result<common::models::PriceOverride>
where
    E: PgExecutor<'e>,
{
    let record = sqlx::query_as::<_, crate::models::PriceOverride>(
        r#"
        INSERT INTO listing_price_overrides (
            id, listing_id, start_date, end_date, nightly_rate, min_nights
        )
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, listing_id, start_date, end_date, nightly_rate, min_nights, created_at, updated_at
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(listing_id)
    .bind(req.start_date)
    .bind(req.end_date)
    .bind(req.nightly_rate)
    .bind(req.min_nights)
    .fetch_one(executor)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23P01") => {
            crate::error::DbError::ValidationError(
                "Overlapping price override date range exists for this listing".to_string(),
            )
        }
        other => crate::error::DbError::Sqlx(other),
    })?;

    Ok(record.into())
}

/// Retrieves all price overrides for a listing sorted by start_date ascending.
#[tracing::instrument(skip(executor))]
pub async fn get_price_overrides_by_listing<'e, E>(
    executor: E,
    listing_id: Uuid,
) -> Result<Vec<common::models::PriceOverride>>
where
    E: PgExecutor<'e>,
{
    let records = sqlx::query_as::<_, crate::models::PriceOverride>(
        r#"
        SELECT id, listing_id, start_date, end_date, nightly_rate, min_nights, created_at, updated_at
        FROM listing_price_overrides
        WHERE listing_id = $1
        ORDER BY start_date ASC
        "#,
    )
    .bind(listing_id)
    .fetch_all(executor)
    .await?;

    Ok(records.into_iter().map(Into::into).collect())
}

/// Retrieves active price overrides for a stay window [check_in, check_out).
#[tracing::instrument(skip(executor))]
pub async fn get_active_overrides_for_dates<'e, E>(
    executor: E,
    listing_id: Uuid,
    check_in: chrono::NaiveDate,
    check_out: chrono::NaiveDate,
) -> Result<Vec<common::models::PriceOverride>>
where
    E: PgExecutor<'e>,
{
    let records = sqlx::query_as::<_, crate::models::PriceOverride>(
        r#"
        SELECT id, listing_id, start_date, end_date, nightly_rate, min_nights, created_at, updated_at
        FROM listing_price_overrides
        WHERE listing_id = $1
          AND start_date < $3
          AND end_date > $2
        ORDER BY start_date ASC
        "#,
    )
    .bind(listing_id)
    .bind(check_in)
    .bind(check_out)
    .fetch_all(executor)
    .await?;

    Ok(records.into_iter().map(Into::into).collect())
}

/// Retrieves a single price override by ID.
#[tracing::instrument(skip(executor))]
pub async fn get_price_override_by_id<'e, E>(
    executor: E,
    override_id: Uuid,
    listing_id: Uuid,
) -> Result<common::models::PriceOverride>
where
    E: PgExecutor<'e>,
{
    let record = sqlx::query_as::<_, crate::models::PriceOverride>(
        r#"
        SELECT id, listing_id, start_date, end_date, nightly_rate, min_nights, created_at, updated_at
        FROM listing_price_overrides
        WHERE id = $1 AND listing_id = $2
        "#,
    )
    .bind(override_id)
    .bind(listing_id)
    .fetch_one(executor)
    .await?;

    Ok(record.into())
}

/// Updates an existing price override.
#[tracing::instrument(skip(executor))]
pub async fn update_price_override<'e, E>(
    executor: E,
    override_id: Uuid,
    listing_id: Uuid,
    req: &common::models::UpdatePriceOverrideRequest,
) -> Result<common::models::PriceOverride>
where
    E: PgExecutor<'e>,
{
    let record = sqlx::query_as::<_, crate::models::PriceOverride>(
        r#"
        UPDATE listing_price_overrides
        SET start_date = COALESCE($3, start_date),
            end_date = COALESCE($4, end_date),
            nightly_rate = COALESCE($5, nightly_rate),
            min_nights = COALESCE($6, min_nights),
            updated_at = NOW()
        WHERE id = $1 AND listing_id = $2
        RETURNING id, listing_id, start_date, end_date, nightly_rate, min_nights, created_at, updated_at
        "#,
    )
    .bind(override_id)
    .bind(listing_id)
    .bind(req.start_date)
    .bind(req.end_date)
    .bind(req.nightly_rate)
    .bind(req.min_nights)
    .fetch_one(executor)
    .await
    .map_err(|e| match e {
        sqlx::Error::Database(db_err) if db_err.code().as_deref() == Some("23P01") => {
            crate::error::DbError::ValidationError(
                "Overlapping price override date range exists for this listing".to_string(),
            )
        }
        other => crate::error::DbError::Sqlx(other),
    })?;

    Ok(record.into())
}

/// Deletes a price override.
#[tracing::instrument(skip(executor))]
pub async fn delete_price_override<'e, E>(
    executor: E,
    override_id: Uuid,
    listing_id: Uuid,
) -> Result<()>
where
    E: PgExecutor<'e>,
{
    let result = sqlx::query(
        r#"
        DELETE FROM listing_price_overrides
        WHERE id = $1 AND listing_id = $2
        "#,
    )
    .bind(override_id)
    .bind(listing_id)
    .execute(executor)
    .await?;

    if result.rows_affected() == 0 {
        return Err(crate::error::DbError::ValidationError(
            "Price override not found".to_string(),
        ));
    }

    Ok(())
}
