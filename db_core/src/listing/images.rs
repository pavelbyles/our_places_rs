//! Listing images: upload presigns, processing state, variants and deletion.

use super::*;

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

/// Deletes a listing photo and its resized variants (they cascade). If it
/// was the primary photo, the first remaining photo (processed ones first)
/// becomes primary. A photo that is not on `listing_id` is `RowNotFound`.
#[tracing::instrument(skip(tx))]
pub async fn delete_listing_image(
    tx: &mut sqlx::PgConnection,
    listing_id: Uuid,
    image_id: Uuid,
) -> Result<()> {
    let was_primary: bool = sqlx::query_scalar(
        r#"
        DELETE FROM listing_image
        WHERE id = $1 AND listing_id = $2 AND parent_id IS NULL
        RETURNING is_primary
        "#,
    )
    .bind(image_id)
    .bind(listing_id)
    .fetch_one(&mut *tx)
    .await?;

    if was_primary {
        sqlx::query(
            r#"
            UPDATE listing_image SET is_primary = TRUE, updated_at = NOW()
            WHERE id = (
                SELECT id FROM listing_image
                WHERE listing_id = $1 AND parent_id IS NULL
                ORDER BY (status = 'Processed') DESC, display_order, created_at
                LIMIT 1
            )
            "#,
        )
        .bind(listing_id)
        .execute(&mut *tx)
        .await?;
    }

    Ok(())
}
