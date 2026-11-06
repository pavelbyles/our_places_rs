//! Reading listings: search, by owner, by id or slug.

use crate::error::Result;
use crate::models::{Listing, ListingWithOwner};
use sqlx::PgExecutor;
use uuid::Uuid;

/// Makes text match literally in a `LIKE` pattern using `ESCAPE '\\'`.
fn escape_like(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Limits a search to listings that can be booked for the whole stay: active,
/// priced, long enough to meet the minimum stay (including overrides covering a
/// night of it), and clear of bookings under the rule in `booking::availability`.
fn push_stay_conditions(
    query_builder: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>,
    check_in: chrono::NaiveDate,
    check_out: chrono::NaiveDate,
) {
    let nights = i32::try_from((check_out - check_in).num_days()).unwrap_or(i32::MAX);
    query_builder.push(
        " AND listing.is_active = TRUE AND listing.price_per_night IS NOT NULL AND listing.minimum_stay <= ",
    );
    query_builder.push_bind(nights);
    query_builder.push(
        " AND NOT EXISTS (SELECT 1 FROM listing_price_overrides o
            WHERE o.listing_id = listing.id AND o.start_date < ",
    );
    query_builder.push_bind(check_out);
    query_builder.push(" AND o.end_date > ");
    query_builder.push_bind(check_in);
    query_builder.push(" AND o.min_nights > ");
    query_builder.push_bind(nights);
    query_builder.push(
        ") AND NOT EXISTS (SELECT 1 FROM booking b
            WHERE b.listing_id = listing.id
              AND (b.status = 'confirmed' OR (b.status = 'pending' AND b.expires_at > NOW()))
              AND b.date_from - LEAST(GREATEST(listing.days_between_bookings, 0), 365) < ",
    );
    query_builder.push_bind(check_out);
    query_builder
        .push(" AND b.date_to + LEAST(GREATEST(listing.days_between_bookings, 0), 365) > ");
    query_builder.push_bind(check_in);
    // Live requests to book (#105) block like holds.
    query_builder.push(
        ") AND NOT EXISTS (SELECT 1 FROM booking_request r
            WHERE r.listing_id = listing.id
              AND r.status IN ('open', 'accepted', 'quoted') AND r.expires_at > NOW()
              AND r.date_from - LEAST(GREATEST(listing.days_between_bookings, 0), 365) < ",
    );
    query_builder.push_bind(check_out);
    query_builder
        .push(" AND r.date_to + LEAST(GREATEST(listing.days_between_bookings, 0), 365) > ");
    query_builder.push_bind(check_in);
    query_builder.push(")");
}

/// Retrieves a paginated list of listings from the database with optional filtering.
/// The filter stays out of the span, because it can hold what a guest typed.
#[tracing::instrument(skip(executor, filter))]
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

    if !filter.as_ref().is_some_and(|f| f.include_inactive) {
        query_builder.push(" AND listing.is_active = TRUE");
    }

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

        if let (Some(check_in), Some(check_out)) = (f.check_in, f.check_out) {
            push_stay_conditions(&mut query_builder, check_in, check_out);
        }

        if let Some(q) = f.q.as_deref() {
            let pattern = format!("%{}%", escape_like(q));
            query_builder.push(" AND (listing.name ILIKE ");
            query_builder.push_bind(pattern.clone());
            query_builder.push(" ESCAPE '\\' OR listing.city ILIKE ");
            query_builder.push_bind(pattern.clone());
            query_builder.push(" ESCAPE '\\' OR listing.country ILIKE ");
            query_builder.push_bind(pattern);
            query_builder.push(" ESCAPE '\\')");
        }

        if let Some(guests) = f.guests {
            query_builder.push(" AND listing.max_guests >= ");
            query_builder.push_bind(guests);
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
