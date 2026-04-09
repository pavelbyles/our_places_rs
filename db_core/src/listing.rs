use crate::error::Result;
use crate::models::{Listing, ListingWithOwner, NewListing, UpdatedListing};
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

pub fn generate_listing_slug_native(title: &str) -> String {
    const MAX_SLUG_LEN: usize = 60;
    let mut slug = String::with_capacity(MAX_SLUG_LEN);
    let mut last_was_hyphen = false;

    for c in title.to_lowercase().chars() {
        if slug.len() >= MAX_SLUG_LEN {
            break;
        }

        if c.is_ascii_alphanumeric() {
            slug.push(c);
            last_was_hyphen = false;
        } else if !last_was_hyphen && !slug.is_empty() {
            slug.push('-');
            last_was_hyphen = true;
        }
    }

    let trimmed = slug.trim_end_matches('-');

    if Uuid::parse_str(trimmed).is_ok() {
        format!("v-{}", trimmed)
    } else {
        trimmed.to_string()
    }
}

/// Creates a new listing in the database.
#[tracing::instrument(skip(executor))]
pub async fn create_listing<'e, E>(executor: E, new_listing: &NewListing) -> Result<Listing>
where
    E: PgExecutor<'e>,
{
    let listing = sqlx::query_as!(
        Listing,
        r#"
        INSERT INTO listing (
            id, user_id, name, description, listing_structure_id, country, price_per_night, 
            weekly_discount_percentage, monthly_discount_percentage, added_at, slug, 
            max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, square_meters, 
            latitude, longitude, listing_details, overall_rating, review_count, city, base_currency
        )
        SELECT $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, COALESCE($20, '{}'::jsonb), $21, $22, $23, $24
        WHERE EXISTS (SELECT 1 FROM host_profiles WHERE user_id = $2)
        RETURNING 
            id, user_id, name, description, listing_structure_id, country, price_per_night, 
            is_active, added_at, deleted_at, CAST(NULL AS TEXT) as primary_image_url, 
            weekly_discount_percentage, monthly_discount_percentage, slug, 
            max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, square_meters, 
            latitude, longitude, CAST(overall_rating AS FLOAT8) as overall_rating, review_count, listing_details, city, base_currency
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
        SELECT listing.id, listing.user_id, listing.name, listing.description, listing.listing_structure_id, listing.country, listing.price_per_night, listing.is_active, listing.added_at, listing.deleted_at, listing.weekly_discount_percentage, listing.monthly_discount_percentage, listing.max_guests, listing.bedrooms, listing.full_bathrooms, listing.latitude, listing.longitude, CAST(listing.overall_rating AS FLOAT8) as overall_rating, listing.city, listing.slug, listing.base_currency,
        "user".first_name || ' ' || "user".last_name as owner_name,
        primary_img.upload_url as primary_image_url
        FROM listing
        INNER JOIN "user" ON listing.user_id = "user".id
        LEFT JOIN LATERAL (
            SELECT thumb_img.upload_url
            FROM listing_image parent_img
            JOIN listing_image thumb_img ON thumb_img.parent_id = parent_img.id
            WHERE parent_img.listing_id = listing.id
              AND parent_img.is_primary = TRUE
              AND thumb_img.resolution = "#,
    );
    query_builder.push_bind(parsed_res);
    query_builder.push(
        r#"
            LIMIT 1
        ) AS primary_img ON true
        WHERE listing.deleted_at IS NULL
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

        if let Some(owner) = f.owner {
            query_builder.push(" AND \"user\".email ILIKE ");
            query_builder.push_bind(format!("%{}%", owner));
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
        SELECT listing.id, listing.user_id, listing.name, listing.description, listing.listing_structure_id, listing.country, listing.price_per_night, listing.is_active, listing.added_at, listing.deleted_at, listing.weekly_discount_percentage, listing.monthly_discount_percentage, primary_img.upload_url as primary_image_url, listing.slug, listing.max_guests, listing.bedrooms, listing.beds, listing.full_bathrooms, listing.half_bathrooms, listing.square_meters, listing.latitude, listing.longitude, CAST(listing.overall_rating AS FLOAT8) as overall_rating, listing.review_count, listing.listing_details, listing.city, listing.base_currency
        FROM listing
        LEFT JOIN LATERAL (
            SELECT thumb_img.upload_url
            FROM listing_image parent_img
            JOIN listing_image thumb_img ON thumb_img.parent_id = parent_img.id
            WHERE parent_img.listing_id = listing.id
              AND parent_img.is_primary = TRUE
              AND thumb_img.resolution = 'Thumbnail400w'::image_resolution
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
pub async fn get_listing_by_id<'a, A>(executor: A, id: Uuid) -> Result<crate::models::ListingDetails>
where
    A: sqlx::Acquire<'a, Database = sqlx::Postgres>,
{
    let mut conn = executor.acquire().await?;

    let listing = sqlx::query_as!(
        Listing,
        r#"
        SELECT listing.id, listing.user_id, listing.name, listing.description, listing.listing_structure_id, listing.country, listing.price_per_night, listing.is_active, listing.added_at, listing.deleted_at, listing.weekly_discount_percentage, listing.monthly_discount_percentage, primary_img.upload_url as primary_image_url, listing.slug, listing.max_guests, listing.bedrooms, listing.beds, listing.full_bathrooms, listing.half_bathrooms, listing.square_meters, listing.latitude, listing.longitude, CAST(listing.overall_rating AS FLOAT8) as overall_rating, listing.review_count, listing.listing_details, listing.city, listing.base_currency
        FROM listing
        LEFT JOIN LATERAL (
            SELECT thumb_img.upload_url
            FROM listing_image parent_img
            JOIN listing_image thumb_img ON thumb_img.parent_id = parent_img.id
            WHERE parent_img.listing_id = listing.id
              AND parent_img.is_primary = TRUE
              AND thumb_img.resolution = 'Thumbnail400w'::image_resolution
            LIMIT 1
        ) AS primary_img ON true
        WHERE listing.id = $1 AND listing.deleted_at IS NULL
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

    Ok(crate::models::ListingDetails { listing, images })
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

    let is_uuid = uuid::Uuid::parse_str(id_or_slug).is_ok();

    let listing = if is_uuid {
        let id_uuid = uuid::Uuid::parse_str(id_or_slug).unwrap();
        sqlx::query_as!(
            Listing,
            r#"
            SELECT listing.id, listing.user_id, listing.name, listing.description, listing.listing_structure_id, listing.country, listing.price_per_night, listing.is_active, listing.added_at, listing.deleted_at, listing.weekly_discount_percentage, listing.monthly_discount_percentage, primary_img.upload_url as primary_image_url, listing.slug, listing.max_guests, listing.bedrooms, listing.beds, listing.full_bathrooms, listing.half_bathrooms, listing.square_meters, listing.latitude, listing.longitude, CAST(listing.overall_rating AS FLOAT8) as overall_rating, listing.review_count, listing.listing_details, listing.city, listing.base_currency
            FROM listing
            LEFT JOIN LATERAL (
                SELECT thumb_img.upload_url
                FROM listing_image parent_img
                JOIN listing_image thumb_img ON thumb_img.parent_id = parent_img.id
                WHERE parent_img.listing_id = listing.id
                  AND parent_img.is_primary = TRUE
                  AND thumb_img.resolution = 'Thumbnail400w'::image_resolution
                LIMIT 1
            ) AS primary_img ON true
            WHERE listing.id = $1 AND listing.deleted_at IS NULL
            "#,
            id_uuid
        )
        .fetch_one(&mut *conn)
        .await?
    } else {
        sqlx::query_as!(
            Listing,
            r#"
            SELECT listing.id, listing.user_id, listing.name, listing.description, listing.listing_structure_id, listing.country, listing.price_per_night, listing.is_active, listing.added_at, listing.deleted_at, listing.weekly_discount_percentage, listing.monthly_discount_percentage, primary_img.upload_url as primary_image_url, listing.slug, listing.max_guests, listing.bedrooms, listing.beds, listing.full_bathrooms, listing.half_bathrooms, listing.square_meters, listing.latitude, listing.longitude, CAST(listing.overall_rating AS FLOAT8) as overall_rating, listing.review_count, listing.listing_details, listing.city, listing.base_currency
            FROM listing
            LEFT JOIN LATERAL (
                SELECT thumb_img.upload_url
                FROM listing_image parent_img
                JOIN listing_image thumb_img ON thumb_img.parent_id = parent_img.id
                WHERE parent_img.listing_id = listing.id
                  AND parent_img.is_primary = TRUE
                  AND thumb_img.resolution = 'Thumbnail400w'::image_resolution
                LIMIT 1
            ) AS primary_img ON true
            WHERE listing.slug = $1 AND listing.deleted_at IS NULL
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

    Ok(crate::models::ListingDetails { listing, images })
}

/// Updates a listing in the database.
#[tracing::instrument(skip(pool))]
pub async fn update_listing(
    pool: &PgPool,
    id: Uuid,
    updated_listing_data: &UpdatedListing,
) -> Result<Listing> {
    let mut tx = pool.begin().await?;

    let _current = sqlx::query_as!(
        Listing,
        r#"SELECT id, user_id, name, description, listing_structure_id, country, price_per_night, is_active, added_at, deleted_at, CAST(NULL AS TEXT) as primary_image_url, weekly_discount_percentage, monthly_discount_percentage, slug, max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, square_meters, latitude, longitude, CAST(overall_rating AS FLOAT8) as overall_rating, review_count, listing_details, city, base_currency FROM listing WHERE id = $1 FOR UPDATE"#,
        id
    )
    .fetch_one(&mut *tx)
    .await?;

    // 2. Archive current state to history via direct table copy to ensure 100% data fidelity
    sqlx::query!(
        r#"
        INSERT INTO listing_history (
            listing_id, user_id, name, description, listing_structure_id, country, 
            price_per_night, is_active, weekly_discount_percentage, monthly_discount_percentage, 
            slug, max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, square_meters, 
            latitude, longitude, overall_rating, review_count, listing_details, valid_from
        )
        SELECT 
            id, user_id, name, description, listing_structure_id, country, 
            price_per_night, is_active, weekly_discount_percentage, monthly_discount_percentage, 
            slug, max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, square_meters, 
            latitude, longitude, overall_rating, review_count, listing_details, added_at
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
            base_currency = COALESCE($20, base_currency)
        WHERE id = $1
        RETURNING id, user_id, name, description, listing_structure_id, country, price_per_night, is_active, added_at, deleted_at, CAST(NULL AS TEXT) as primary_image_url, weekly_discount_percentage, monthly_discount_percentage, slug, max_guests, bedrooms, beds, full_bathrooms, half_bathrooms, square_meters, latitude, longitude, CAST(overall_rating AS FLOAT8) as overall_rating, review_count, listing_details, city, base_currency
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
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

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
            .await?
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

/// Batch inserts pending listing images in preparation for presigned URL uploads.
#[tracing::instrument(skip(executor))]
pub async fn create_listing_image_presigns<'a, A>(
    executor: A,
    listing_id: Uuid,
    images: &[(Uuid, common::models::PendingImageMetadata, String)],
) -> Result<Vec<crate::models::ListingImage>>
where
    A: sqlx::Acquire<'a, Database = sqlx::Postgres>,
{
    if images.is_empty() {
        return Ok(Vec::new());
    }

    let mut conn = executor.acquire().await?;

    let max_order: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(display_order), -1)::integer FROM listing_image WHERE listing_id = $1",
    )
    .bind(listing_id)
    .fetch_one(&mut *conn)
    .await?;

    let min_batch_order = images
        .iter()
        .map(|(_, img, _)| img.display_order)
        .min()
        .unwrap_or(0);
    let mut primary_assigned = max_order > -1;

    let mut query_builder = sqlx::QueryBuilder::new(
        "INSERT INTO listing_image (id, listing_id, client_file_id, status, content_type, size_bytes, display_order, upload_url, is_primary) ",
    );

    query_builder.push_values(images.iter(), |mut b, (id, img, url)| {
        let mut is_primary = false;
        if !primary_assigned && img.display_order == min_batch_order {
            is_primary = true;
            primary_assigned = true;
        }

        b.push_bind(*id)
            .push_bind(listing_id)
            .push_bind(img.client_file_id.clone())
            .push_bind(crate::models::ImageStatus::PendingUpload)
            .push_bind(img.content_type.clone())
            .push_bind(img.size_bytes as i64)
            .push_bind(max_order + 1 + img.display_order)
            .push_bind(url.clone())
            .push_bind(is_primary);
    });

    query_builder.push(" RETURNING *");

    let query = query_builder.build_query_as::<crate::models::ListingImage>();
    let inserted = query.fetch_all(&mut *conn).await?;

    Ok(inserted)
}

/// Updates a listing image to Processing status and records the size/content_type
#[tracing::instrument(skip(executor))]
pub async fn update_listing_image_to_processing<'e, E>(
    executor: E,
    image_id: Uuid,
    size_bytes: i64,
    content_type: String,
) -> Result<crate::models::ListingImage>
where
    E: sqlx::PgExecutor<'e>,
{
    let image = sqlx::query_as!(
        crate::models::ListingImage,
        r#"
        UPDATE listing_image
        SET status = 'Processing', size_bytes = $2, content_type = $3, updated_at = now()
        WHERE id = $1
        RETURNING id, listing_id, client_file_id, status as "status: crate::models::ImageStatus", resolution as "resolution: crate::models::ImageResolution", parent_id, upload_url, content_type, size_bytes, display_order, is_primary, created_at, updated_at
        "#,
        image_id,
        size_bytes,
        content_type
    )
    .fetch_one(executor)
    .await?;

    Ok(image)
}

/// Marks a listing image as Processed, recording its upload URL
#[tracing::instrument(skip(executor))]
pub async fn mark_listing_image_processed<'e, E>(
    executor: E,
    image_id: Uuid,
    public_url: String,
) -> Result<()>
where
    E: sqlx::PgExecutor<'e>,
{
    sqlx::query!(
        r#"
        UPDATE listing_image
        SET status = 'Processed', upload_url = $2, updated_at = now()
        WHERE id = $1
        "#,
        image_id,
        public_url
    )
    .execute(executor)
    .await?;

    Ok(())
}

/// Inserts multiple resized variants for a parent image
#[tracing::instrument(skip(executor))]
pub async fn insert_listing_image_variants<'e, E>(
    executor: E,
    variants: &[(
        Uuid,
        Uuid,
        String,
        crate::models::ImageResolution,
        i64,
        String,
        String,
    )], // (listing_id, parent_id, client_file_id, resolution, size_bytes, content_type, upload_url)
) -> Result<()>
where
    E: sqlx::PgExecutor<'e>,
{
    if variants.is_empty() {
        return Ok(());
    }

    let mut query_builder = sqlx::QueryBuilder::new(
        "INSERT INTO listing_image (listing_id, parent_id, client_file_id, resolution, status, size_bytes, content_type, upload_url) ",
    );

    query_builder.push_values(
        variants.iter(),
        |mut b, (listing_id, parent_id, client_file_id, resolution, size, ctype, url)| {
            b.push_bind(*listing_id)
                .push_bind(*parent_id)
                .push_bind(client_file_id.clone())
                .push_bind(*resolution)
                .push_bind(crate::models::ImageStatus::Processed)
                .push_bind(*size)
                .push_bind(ctype.clone())
                .push_bind(url.clone());
        },
    );

    query_builder.build().execute(executor).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::DbError;
    use crate::models::{NewHostProfile, NewListing, NewUser};
    use crate::user::{create_host_profile, create_user};
    use rust_decimal_macros::dec;
    use sqlx::{Connection, PgConnection};
    use std::collections::HashSet;
    use std::env;
    use uuid::Uuid;

    /// Helper function to establish a fresh connection to the test database.
    async fn setup_test_db() -> PgConnection {
        dotenvy::dotenv().ok();
        let database_url = env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/our_places".to_string()
        });

        PgConnection::connect(&database_url)
            .await
            .expect("Failed to connect to Postgres")
    }

    async fn create_test_user_no_profile(conn: &mut PgConnection) -> Uuid {
        let id = Uuid::now_v7();
        let new_user = NewUser {
            id,
            email: format!("test_{}@example.com", id),
            password_hash: "secret".to_string(),
            first_name: "Test".to_string(),
            last_name: "User".to_string(),
            phone_number: None,
            is_active: true,
            attributes: serde_json::json!({}),
            roles: None,
        };
        create_user(&mut *conn, &new_user)
            .await
            .expect("Failed to create test user");
        id
    }

    async fn create_test_user_with_host_profile(conn: &mut PgConnection) -> Uuid {
        let user_id = create_test_user_no_profile(&mut *conn).await;
        let profile = NewHostProfile {
            description: Some("Test Host".to_string()),
            verified_status: Some("pending".to_string()),
            payout_details: None,
        };
        create_host_profile(&mut *conn, user_id, &profile)
            .await
            .expect("Failed to create host profile");
        user_id
    }

    #[tokio::test]
    async fn test_create_listing() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        let user_id = create_test_user_with_host_profile(&mut *tx).await;
        let new_listing = NewListing {
            name: "Cozy Test Cottage".to_string(),
            user_id,
            description: Some("A beautiful cottage for testing.".to_string()),
            listing_structure_id: 2, // House
            country: "Testland".to_string(),
            price_per_night: Some(dec!(150.75)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 4,
            bedrooms: 2,
            beds: 2,
            full_bathrooms: 1,
            half_bathrooms: 0,
            square_meters: Some(100),
            latitude: None,
            longitude: None,
            listing_details: None,
            city: None,
            base_currency: "USD".to_string(),
        };

        let created_listing = create_listing(&mut *tx, &new_listing).await.unwrap();

        assert_eq!(created_listing.name, new_listing.name);
        assert!(!created_listing.id.is_nil());
        assert!(!created_listing.is_active);
    }

    #[tokio::test]
    async fn test_create_listing_fails_without_host_profile() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        let user_id = create_test_user_no_profile(&mut *tx).await;
        let new_listing = NewListing {
            name: "Fail listing".to_string(),
            user_id,
            description: None,
            listing_structure_id: 1,
            country: "Testland".to_string(),
            price_per_night: None,
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 2,
            bedrooms: 1,
            beds: 1,
            full_bathrooms: 1,
            half_bathrooms: 0,
            square_meters: None,
            latitude: None,
            longitude: None,
            listing_details: None,
            city: None,
            base_currency: "USD".to_string(),
        };

        let result = create_listing(&mut *tx, &new_listing).await;
        assert!(matches!(
            result,
            Err(DbError::ValidationError(msg)) if msg == "User must have a host profile to create a listing"
        ));
    }

    #[tokio::test]
    async fn test_get_listing_by_id() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        let user_id = create_test_user_with_host_profile(&mut *tx).await;
        let new_listing = NewListing {
            name: "Fetch Me Listing".to_string(),
            user_id,
            description: None,
            listing_structure_id: 3,
            country: "Republic of Testing".to_string(),
            price_per_night: Some(dec!(99.99)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 2,
            bedrooms: 1,
            beds: 1,
            full_bathrooms: 1,
            half_bathrooms: 0,
            square_meters: None,
            latitude: None,
            longitude: None,
            listing_details: None,
            city: None,
            base_currency: "USD".to_string(),
        };
        let created_listing = create_listing(&mut *tx, &new_listing).await.unwrap();

        let fetched_listing = get_listing_by_id(&mut *tx, created_listing.id)
            .await
            .unwrap();

        assert_eq!(created_listing.id, fetched_listing.listing.id);

        let non_existent_id = Uuid::now_v7();
        let result = get_listing_by_id(&mut *tx, non_existent_id).await;
        assert!(matches!(
            result,
            Err(DbError::Sqlx(sqlx::Error::RowNotFound))
        ));
    }

    #[tokio::test]
    async fn test_get_listings_pagination() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        // Create 3 listings. We will order them by name for deterministic testing.
        let user_id = create_test_user_with_host_profile(&mut *tx).await;
        let mut created_ids = Vec::new();
        for i in 1..=3 {
            let listing = NewListing {
                name: format!("Pagination Test Listing {}", i),
                user_id,
                description: None,
                listing_structure_id: 1,
                country: "Testland".to_string(),
                price_per_night: None,
                weekly_discount_percentage: None,
                monthly_discount_percentage: None,
                max_guests: 2,
                bedrooms: 1,
                beds: 1,
                full_bathrooms: 1,
                half_bathrooms: 0,
                square_meters: None,
                latitude: None,
                longitude: None,
                listing_details: None,
                city: None,
                base_currency: "USD".to_string(),
            };
            let created = create_listing(&mut *tx, &listing).await.unwrap();
            created_ids.push(created.id);
        }
        // Since we sort by added_at DESC, id DESC, and all have same added_at (transaction time),
        // we need to know the order of IDs to predict page content.
        // However, instead of predicting order, we can check that standard pagination properties hold
        // AND that our items are present in the top results (since they are newest).

        // Page 1 (Limit 2)
        let page1 = get_listings(&mut *tx, 1, 2, None).await.unwrap();
        assert_eq!(page1.len(), 2);

        // Page 2 (Limit 2)
        let page2 = get_listings(&mut *tx, 2, 2, None).await.unwrap();
        // Page 2 might have >1 items if DB had pre-existing data.
        // But we expect at least 1 of ours.

        // Check that our created items are in the first 3 results (across p1 and p2)
        let mut top_3_ids = Vec::new();
        top_3_ids.extend(page1.iter().map(|l| l.id));
        if let Some(l) = page2.first() {
            top_3_ids.push(l.id);
        }

        // Verify all created_ids are in the found top results.
        // Note: Newest items should come first.
        let created_set: HashSet<Uuid> = created_ids.into_iter().collect();
        let found_set: HashSet<Uuid> = top_3_ids.into_iter().collect();

        // We assert that all our created items are found.
        for id in created_set {
            assert!(
                found_set.contains(&id),
                "Created listing {} not found in top pagination results",
                id
            );
        }
    }

    #[tokio::test]
    async fn test_get_listings_uniqueness_across_pages() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        const TOTAL_RECORDS: i32 = 7;
        const PER_PAGE: u32 = 3;

        let user_id = create_test_user_with_host_profile(&mut *tx).await;
        for i in 1..=TOTAL_RECORDS {
            let listing = NewListing {
                name: format!("Uniqueness Test Listing {}", i),
                user_id,
                description: None,
                listing_structure_id: 1,
                country: "Testland".to_string(),
                price_per_night: None,
                weekly_discount_percentage: None,
                monthly_discount_percentage: None,
                max_guests: 2,
                bedrooms: 1,
                beds: 1,
                full_bathrooms: 1,
                half_bathrooms: 0,
                square_meters: None,
                latitude: None,
                longitude: None,
                listing_details: None,
                city: None,
                base_currency: "USD".to_string(),
            };
            create_listing(&mut *tx, &listing).await.unwrap();
        }

        let mut all_fetched_listings = Vec::new();
        let mut current_page = 1;
        loop {
            let page_results = get_listings(&mut *tx, current_page, PER_PAGE, None)
                .await
                .unwrap();
            if page_results.is_empty() {
                break;
            }
            all_fetched_listings.extend(page_results);
            current_page += 1;
        }

        assert!(
            all_fetched_listings.len() as i32 >= TOTAL_RECORDS,
            "Pagination should return at least the inserted number of listings"
        );

        let ids: Vec<Uuid> = all_fetched_listings.iter().map(|l| l.id).collect();
        let unique_ids: HashSet<Uuid> = ids.iter().cloned().collect();

        assert_eq!(
            ids.len(),
            unique_ids.len(),
            "Found duplicate listings across different pages"
        );
    }

    #[tokio::test]
    async fn test_get_listings_filtering() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        let user_id = create_test_user_with_host_profile(&mut *tx).await;

        // 1. Apartment in Jamaica, cheap
        let listing1 = NewListing {
            name: "Cheap Apartment Jamaica".to_string(),
            user_id,
            description: None,
            listing_structure_id: 1, // Apartment
            country: "Jamaica".to_string(),
            price_per_night: Some(dec!(50.00)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 2,
            bedrooms: 1,
            beds: 1,
            full_bathrooms: 1,
            half_bathrooms: 0,
            square_meters: None,
            latitude: None,
            longitude: None,
            listing_details: None,
            city: None,
            base_currency: "USD".to_string(),
        };
        create_listing(&mut *tx, &listing1).await.unwrap();

        // 2. Villa in Jamaica, expensive
        let listing2 = NewListing {
            name: "Luxury Villa Jamaica".to_string(),
            user_id,
            description: None,
            listing_structure_id: 5, // Villa
            country: "Jamaica".to_string(),
            price_per_night: Some(dec!(500.00)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 6,
            bedrooms: 3,
            beds: 3,
            full_bathrooms: 3,
            half_bathrooms: 1,
            square_meters: None,
            latitude: None,
            longitude: None,
            listing_details: None,
            city: None,
            base_currency: "USD".to_string(),
        };
        create_listing(&mut *tx, &listing2).await.unwrap();

        // 3. Apartment in USA, cheap
        let listing3 = NewListing {
            name: "Cheap Apartment USA".to_string(),
            user_id,
            description: None,
            listing_structure_id: 1, // Apartment
            country: "USA".to_string(),
            price_per_night: Some(dec!(60.00)),
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 2,
            bedrooms: 1,
            beds: 1,
            full_bathrooms: 1,
            half_bathrooms: 0,
            square_meters: None,
            latitude: None,
            longitude: None,
            listing_details: None,
            city: None,
            base_currency: "USD".to_string(),
        };
        create_listing(&mut *tx, &listing3).await.unwrap();

        // Test Filter by Country (Jamaica)
        let filter_jamaica = common::models::ListingFilter {
            name: None,
            country: Some("Jamaica".to_string()),
            min_price: None,
            max_price: None,
            structure_type: vec![],
            owner: None,
            resolution: None,
        };
        let results = get_listings(&mut *tx, 1, 10, Some(filter_jamaica))
            .await
            .unwrap();
        // Should find listing1 and listing2 (we might have other data, but these surely)
        // We filter results locally to verify *our* created ones are there only if they match
        let found_names: Vec<String> = results.iter().map(|l| l.name.clone()).collect();
        assert!(found_names.contains(&"Cheap Apartment Jamaica".to_string()));
        assert!(found_names.contains(&"Luxury Villa Jamaica".to_string()));
        assert!(!found_names.contains(&"Cheap Apartment USA".to_string()));

        // Test Filter by Price (< $100)
        let filter_price = common::models::ListingFilter {
            name: None,
            country: None,
            min_price: None,
            max_price: Some(dec!(100.00)),
            structure_type: vec![],
            owner: None,
            resolution: None,
        };
        let results = get_listings(&mut *tx, 1, 10, Some(filter_price))
            .await
            .expect("Failed to fetch listings");

        let found_names_price: Vec<String> = results.iter().map(|l| l.name.clone()).collect();
        assert!(found_names_price.contains(&"Cheap Apartment Jamaica".to_string()));
        assert!(found_names_price.contains(&"Cheap Apartment USA".to_string()));

        // 2. Filter by Structure Type (Villa)
        let filter_villa = common::models::ListingFilter {
            name: None,
            country: None,
            min_price: None,
            max_price: None,
            structure_type: vec!["Villa".to_string()],
            owner: None,
            resolution: None,
        };
        let results = get_listings(&mut *tx, 1, 10, Some(filter_villa))
            .await
            .unwrap();
        let found_names: Vec<String> = results.iter().map(|l| l.name.clone()).collect();
        assert!(found_names.contains(&"Luxury Villa Jamaica".to_string()));
        assert!(!found_names.contains(&"Cheap Apartment Jamaica".to_string()));
    }

    #[tokio::test]
    async fn test_update_listing_image_to_processing() {
        let mut conn = setup_test_db().await;
        let mut tx = conn.begin().await.expect("Failed to begin transaction");

        let user_id = create_test_user_with_host_profile(&mut *tx).await;
        let listing = NewListing {
            name: "Test Image Processing Listing".to_string(),
            user_id,
            description: None,
            listing_structure_id: 1,
            country: "Testland".to_string(),
            price_per_night: None,
            weekly_discount_percentage: None,
            monthly_discount_percentage: None,
            max_guests: 2,
            bedrooms: 1,
            beds: 1,
            full_bathrooms: 1,
            half_bathrooms: 0,
            square_meters: None,
            latitude: None,
            longitude: None,
            listing_details: None,
            city: None,
            base_currency: "USD".to_string(),
        };
        let created_listing = create_listing(&mut *tx, &listing).await.unwrap();

        let image_md = common::models::PendingImageMetadata {
            client_file_id: "test-file-123".to_string(),
            content_type: "image/jpeg".to_string(),
            size_bytes: 1024,
            display_order: 0,
        };
        let images = vec![(Uuid::now_v7(), image_md, "https://upload.url".to_string())];
        let presigns = create_listing_image_presigns(&mut *tx, created_listing.id, &images)
            .await
            .unwrap();

        let image_id = presigns[0].id;

        let result =
            update_listing_image_to_processing(&mut *tx, image_id, 2048, "image/webp".to_string())
                .await;

        match result {
            Ok(_) => println!("Successfully updated image"),
            Err(e) => panic!("Database error occurred: {:?}", e),
        }
    }
}
