//! Reading listings: search, by owner, by id or slug.

use crate::error::Result;
use crate::models::{Listing, ListingWithOwner};
use sqlx::PgExecutor;
use uuid::Uuid;

/// Retrieves a paginated list of listings from the database with optional filtering.
#[tracing::instrument(skip(executor))]
pub async fn get_listings<'e, E>(
    executor: E,
    page: u32,
    per_page: u32,
    filter: Option<common::models::ListingFilter>,
) -> Result<Vec<ListingWithOwner>>
where
    E: PgExecutor<'e>,
{
    // Calculate the LIMIT and OFFSET values for the SQL query.
    let limit = per_page as i64;
    let offset = ((page.max(1) - 1) * per_page) as i64;

    let resolution_str = filter
        .as_ref()
        .and_then(|f| f.resolution.clone())
        .unwrap_or_else(|| "Thumbnail400w".to_string());
    let parsed_res = resolution_str
        .parse::<crate::models::ImageResolution>()
        .unwrap_or(crate::models::ImageResolution::Thumbnail400w);

    let mut query_builder = sqlx::QueryBuilder::new(
        r#"
        SELECT listing.id, listing.user_id, listing.name, listing.description, listing.listing_structure_id, listing.country, listing.price_per_night, listing.is_active, listing.added_at, listing.deleted_at, listing.weekly_discount_percentage, listing.monthly_discount_percentage, listing.max_guests, listing.bedrooms, listing.beds, listing.full_bathrooms, listing.half_bathrooms, listing.square_meters, listing.latitude, listing.longitude, CAST(listing.overall_rating AS FLOAT8) as overall_rating, listing.city, listing.slug, listing.base_currency, listing.listing_details, listing.minimum_stay, listing.days_between_bookings, listing.commission_pct, listing.cleaning_fee, listing.damage_deposit, listing.guests_included, listing.extra_guest_fee,
        "user".first_name || ' ' || "user".last_name as owner_name,
        "user".slug as owner_slug,
        primary_img.upload_url as primary_image_url
        FROM listing
        INNER JOIN "user" ON listing.user_id = "user".id
        LEFT JOIN LATERAL (
            SELECT thumb_img.upload_url
            FROM listing_image parent_img
            JOIN listing_image thumb_img ON thumb_img.parent_id = parent_img.id
            WHERE parent_img.listing_id = listing.id
              AND thumb_img.resolution = "#,
    );
    query_builder.push_bind(parsed_res);
    query_builder.push(
        r#"::image_resolution
            ORDER BY parent_img.is_primary DESC, parent_img.display_order
            LIMIT 1
        ) AS primary_img ON true
        WHERE listing.deleted_at IS NULL AND "user".is_active = TRUE
        "#,
    );

    if let Some(f) = filter {
        if let Some(name) = f.name {
            query_builder.push(" AND listing.name ILIKE ");
            query_builder.push_bind(format!("%{}%", name));
        }

        if let Some(country) = f.country {
            query_builder.push(" AND listing.country ILIKE ");
            query_builder.push_bind(format!("%{}%", country));
        }

        if let Some(min_price) = f.min_price {
            query_builder.push(" AND listing.price_per_night >= ");
            query_builder.push_bind(min_price);
        }

        if let Some(max_price) = f.max_price {
            query_builder.push(" AND listing.price_per_night <= ");
            query_builder.push_bind(max_price);
        }

        if !f.structure_type.is_empty() {
            let mut ids = Vec::new();
            for st_str in &f.structure_type {
                if let Ok(st) = st_str.parse::<crate::models::StructureType>() {
                    ids.push(st.id());
                }
            }

            if !ids.is_empty() {
                query_builder.push(" AND listing.listing_structure_id = ANY(");
                query_builder.push_bind(ids);
                query_builder.push(")");
            }
        }

        if let Some(host) = f.host.as_deref().map(str::trim).filter(|h| !h.is_empty()) {
            query_builder.push(" AND \"user\".slug = ");
            query_builder.push_bind(host.to_lowercase());
        }

        if let Some(uid) = f.user_id {
            query_builder.push(" AND listing.user_id = ");
            query_builder.push_bind(uid);
        } else if let Some(owner) = f.owner {
            let trimmed = owner.trim();
            if trimmed.contains('@') {
                query_builder.push(" AND LOWER(\"user\".email) = LOWER(");
                query_builder.push_bind(trimmed.to_string());
                query_builder.push(")");
            } else {
                query_builder.push(" AND \"user\".email ILIKE ");
                query_builder.push_bind(format!("%{}%", trimmed));
            }
        }
    }

    query_builder.push(" ORDER BY listing.added_at DESC, listing.id DESC LIMIT ");
    query_builder.push_bind(limit);
    query_builder.push(" OFFSET ");
    query_builder.push_bind(offset);

    tracing::debug!("Query: {}", query_builder.sql());

    let query = query_builder.build_query_as::<ListingWithOwner>();
    let listings = query.fetch_all(executor).await?;

    Ok(listings)
}

/// Retrieves all listings for a specific user from the database.
#[tracing::instrument(skip(executor))]
pub async fn get_listings_by_user_id<'e, E>(executor: E, user_id: Uuid) -> Result<Vec<Listing>>
where
    E: PgExecutor<'e>,
{
    let listings = sqlx::query_as!(
        Listing,
        r#"
        SELECT listing.id, listing.user_id, listing.name, listing.description, listing.listing_structure_id, listing.country, listing.price_per_night, listing.is_active, listing.added_at, listing.deleted_at, listing.weekly_discount_percentage, listing.monthly_discount_percentage, primary_img.upload_url as primary_image_url, listing.slug, listing.max_guests, listing.bedrooms, listing.beds, listing.full_bathrooms, listing.half_bathrooms, listing.square_meters, listing.latitude, listing.longitude, CAST(listing.overall_rating AS FLOAT8) as overall_rating, listing.review_count, listing.listing_details, listing.city, listing.base_currency, listing.minimum_stay, listing.days_between_bookings, listing.commission_pct, listing.cleaning_fee, listing.damage_deposit, listing.guests_included, listing.extra_guest_fee
        FROM listing
        LEFT JOIN LATERAL (
            SELECT thumb_img.upload_url
            FROM listing_image parent_img
            JOIN listing_image thumb_img ON thumb_img.parent_id = parent_img.id
            WHERE parent_img.listing_id = listing.id
              AND thumb_img.resolution = 'Thumbnail400w'::image_resolution
            ORDER BY parent_img.is_primary DESC, parent_img.display_order
            LIMIT 1
        ) AS primary_img ON true
        WHERE listing.user_id = $1 AND listing.deleted_at IS NULL
        ORDER BY listing.added_at DESC, listing.id DESC
        "#,
        user_id
    )
    .fetch_all(executor)
    .await?;

    Ok(listings)
}

/// Retrieves a single listing from the database by its UUID.
#[tracing::instrument(skip(executor))]
pub async fn get_listing_by_id<'a, A>(
    executor: A,
    id: Uuid,
) -> Result<crate::models::ListingDetails>
where
    A: sqlx::Acquire<'a, Database = sqlx::Postgres>,
{
    let mut conn = executor.acquire().await?;

    let listing = sqlx::query_as!(
        Listing,
        r#"
        SELECT listing.id, listing.user_id, listing.name, listing.description, listing.listing_structure_id, listing.country, listing.price_per_night, listing.is_active, listing.added_at, listing.deleted_at, listing.weekly_discount_percentage, listing.monthly_discount_percentage, primary_img.upload_url as primary_image_url, listing.slug, listing.max_guests, listing.bedrooms, listing.beds, listing.full_bathrooms, listing.half_bathrooms, listing.square_meters, listing.latitude, listing.longitude, CAST(listing.overall_rating AS FLOAT8) as overall_rating, listing.review_count, listing.listing_details, listing.city, listing.base_currency, listing.minimum_stay, listing.days_between_bookings, listing.commission_pct, listing.cleaning_fee, listing.damage_deposit, listing.guests_included, listing.extra_guest_fee
        FROM listing
        LEFT JOIN LATERAL (
            SELECT thumb_img.upload_url
            FROM listing_image parent_img
            JOIN listing_image thumb_img ON thumb_img.parent_id = parent_img.id
            WHERE parent_img.listing_id = listing.id
              AND thumb_img.resolution = 'Thumbnail400w'::image_resolution
            ORDER BY parent_img.is_primary DESC, parent_img.display_order
            LIMIT 1
        ) AS primary_img ON true
        INNER JOIN "user" ON listing.user_id = "user".id
        WHERE listing.id = $1 AND listing.deleted_at IS NULL AND "user".is_active = TRUE
        "#,
        id
    )
    .fetch_one(&mut *conn)
    .await?;

    let images = sqlx::query_as!(
        crate::models::ListingImage,
        r#"
        SELECT id, listing_id, client_file_id, status as "status: crate::models::ImageStatus", resolution as "resolution: crate::models::ImageResolution", parent_id, upload_url, content_type, size_bytes, display_order, is_primary, created_at, updated_at
        FROM listing_image
        WHERE listing_id = $1 AND status = 'Processed'
        ORDER BY display_order ASC
        "#,
        id
    )
    .fetch_all(&mut *conn)
    .await?;

    let owner_name: Option<String> =
        sqlx::query_scalar::<_, String>(r#"SELECT first_name FROM "user" WHERE id = $1"#)
            .bind(listing.user_id)
            .fetch_optional(&mut *conn)
            .await?;

    let rating_summary = crate::review::get_listing_rating_summary(&mut conn, id).await?;

    Ok(crate::models::ListingDetails {
        listing,
        images,
        owner_name,
        rating_summary: Some(rating_summary),
    })
}

/// Retrieves a single listing from the database by its UUID or Slug.
#[tracing::instrument(skip(executor))]
pub async fn get_listing_by_id_or_slug<'a, A>(
    executor: A,
    id_or_slug: &str,
) -> Result<crate::models::ListingDetails>
where
    A: sqlx::Acquire<'a, Database = sqlx::Postgres>,
{
    let mut conn = executor.acquire().await?;

    let listing = if let Ok(id_uuid) = uuid::Uuid::parse_str(id_or_slug) {
        sqlx::query_as!(
            Listing,
            r#"
            SELECT listing.id, listing.user_id, listing.name, listing.description, listing.listing_structure_id, listing.country, listing.price_per_night, listing.is_active, listing.added_at, listing.deleted_at, listing.weekly_discount_percentage, listing.monthly_discount_percentage, primary_img.upload_url as primary_image_url, listing.slug, listing.max_guests, listing.bedrooms, listing.beds, listing.full_bathrooms, listing.half_bathrooms, listing.square_meters, listing.latitude, listing.longitude, CAST(listing.overall_rating AS FLOAT8) as overall_rating, listing.review_count, listing.listing_details, listing.city, listing.base_currency, listing.minimum_stay, listing.days_between_bookings, listing.commission_pct, listing.cleaning_fee, listing.damage_deposit, listing.guests_included, listing.extra_guest_fee
            FROM listing
            LEFT JOIN LATERAL (
                SELECT thumb_img.upload_url
                FROM listing_image parent_img
                JOIN listing_image thumb_img ON thumb_img.parent_id = parent_img.id
                WHERE parent_img.listing_id = listing.id
                  AND thumb_img.resolution = 'Thumbnail400w'::image_resolution
                ORDER BY parent_img.is_primary DESC, parent_img.display_order
                LIMIT 1
            ) AS primary_img ON true
            INNER JOIN "user" ON listing.user_id = "user".id
            WHERE listing.id = $1 AND listing.deleted_at IS NULL AND "user".is_active = TRUE
            "#,
            id_uuid
        )
        .fetch_one(&mut *conn)
        .await?
    } else {
        sqlx::query_as!(
            Listing,
            r#"
            SELECT listing.id, listing.user_id, listing.name, listing.description, listing.listing_structure_id, listing.country, listing.price_per_night, listing.is_active, listing.added_at, listing.deleted_at, listing.weekly_discount_percentage, listing.monthly_discount_percentage, primary_img.upload_url as primary_image_url, listing.slug, listing.max_guests, listing.bedrooms, listing.beds, listing.full_bathrooms, listing.half_bathrooms, listing.square_meters, listing.latitude, listing.longitude, CAST(listing.overall_rating AS FLOAT8) as overall_rating, listing.review_count, listing.listing_details, listing.city, listing.base_currency, listing.minimum_stay, listing.days_between_bookings, listing.commission_pct, listing.cleaning_fee, listing.damage_deposit, listing.guests_included, listing.extra_guest_fee
            FROM listing
            LEFT JOIN LATERAL (
                SELECT thumb_img.upload_url
                FROM listing_image parent_img
                JOIN listing_image thumb_img ON thumb_img.parent_id = parent_img.id
                WHERE parent_img.listing_id = listing.id
                  AND thumb_img.resolution = 'Thumbnail400w'::image_resolution
                ORDER BY parent_img.is_primary DESC, parent_img.display_order
                LIMIT 1
            ) AS primary_img ON true
            INNER JOIN "user" ON listing.user_id = "user".id
            WHERE listing.slug = $1 AND listing.deleted_at IS NULL AND "user".is_active = TRUE
            "#,
            id_or_slug
        )
        .fetch_one(&mut *conn)
        .await?
    };

    let images = sqlx::query_as!(
        crate::models::ListingImage,
        r#"
        SELECT id, listing_id, client_file_id, status as "status: crate::models::ImageStatus", resolution as "resolution: crate::models::ImageResolution", parent_id, upload_url, content_type, size_bytes, display_order, is_primary, created_at, updated_at
        FROM listing_image
        WHERE listing_id = $1 AND status = 'Processed'
        ORDER BY display_order ASC
        "#,
        listing.id
    )
    .fetch_all(&mut *conn)
    .await?;

    let owner_name: Option<String> =
        sqlx::query_scalar::<_, String>(r#"SELECT first_name FROM "user" WHERE id = $1"#)
            .bind(listing.user_id)
            .fetch_optional(&mut *conn)
            .await?;

    let rating_summary = crate::review::get_listing_rating_summary(&mut conn, listing.id).await?;

    Ok(crate::models::ListingDetails {
        listing,
        images,
        owner_name,
        rating_summary: Some(rating_summary),
    })
}
