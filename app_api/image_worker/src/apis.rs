use actix_web::middleware::from_fn;
use actix_web::{Error, HttpRequest, HttpResponse, Responder, web};
use api_core::api_common::content_negotiation_middleware;
use google_cloud_storage::client::Storage;
use serde::Deserialize;
use utoipa::{OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;
#[derive(Debug, Deserialize, ToSchema)]
pub struct PubSubPayload {
    pub message: PubSubMessage,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PubSubMessage {
    pub _attributes: Option<PubSubAttributes>,
    pub data: String,
    pub _message_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PubSubAttributes {
    pub _bucket_id: Option<String>,
    pub _event_time: Option<String>,
    pub _event_type: Option<String>,
    pub _notification_config: Option<String>,
    pub _object_generation: Option<String>,
    pub _object_id: Option<String>,
    pub _payload_format: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GcsObjectMetadata {
    pub _kind: Option<String>,
    pub _id: Option<String>,
    pub _self_link: Option<String>,
    pub name: String,
    pub bucket: String,
    pub _generation: Option<String>,
    pub _metageneration: Option<String>,
    pub content_type: Option<String>,
    pub _time_created: Option<String>,
    pub _updated: Option<String>,
    pub _storage_class: Option<String>,
    pub _time_storage_class_updated: Option<String>,
    pub _size: Option<String>,
    pub _md5_hash: Option<String>,
    pub _media_link: Option<String>,
    pub _crc32c: Option<String>,
    pub _etag: Option<String>,
}

use base64::Engine;

impl PubSubMessage {
    pub fn decode_data(&self) -> Result<GcsObjectMetadata, Box<dyn std::error::Error>> {
        let decoded = base64::engine::general_purpose::STANDARD.decode(&self.data)?;
        let metadata: GcsObjectMetadata = serde_json::from_slice(&decoded)?;
        Ok(metadata)
    }
}

pub struct Initialized;
pub struct Downloaded(pub Vec<u8>);
pub struct Processed(pub Vec<(u32, &'static str, db_core::models::ImageResolution, Vec<u8>)>);

pub struct ImageTask<State> {
    pub listing_id: Uuid,
    pub image_id: Uuid,
    pub state: State,
}

impl ImageTask<Initialized> {
    pub fn new(listing_id: Uuid, image_id: Uuid) -> Self {
        Self {
            listing_id,
            image_id,
            state: Initialized,
        }
    }

    pub async fn download(
        self,
        client: &Storage,
        bucket: &str,
        object_name: &str,
    ) -> Result<ImageTask<Downloaded>, actix_web::Error> {
        let gcs_read_bucket = format!("projects/_/buckets/{}", bucket);
        let mut stream_resp = client
            .read_object(&gcs_read_bucket, object_name)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to start download from GCS: {}", e);
                actix_web::error::ErrorInternalServerError("Failed to download image")
            })?;

        let mut data = Vec::new();
        while let Some(chunk_res) = stream_resp.next().await {
            match chunk_res {
                Ok(chunk) => data.extend_from_slice(&chunk),
                Err(e) => {
                    tracing::error!("Error reading chunk from GCS: {}", e);
                    return Err(actix_web::error::ErrorInternalServerError("Download error"));
                }
            }
        }

        Ok(ImageTask {
            listing_id: self.listing_id,
            image_id: self.image_id,
            state: Downloaded(data),
        })
    }
}

impl ImageTask<Downloaded> {
    pub fn process(self) -> Result<ImageTask<Processed>, actix_web::Error> {
        let img = image::load_from_memory(&self.state.0).map_err(|e| {
            tracing::error!("Failed to decode image data: {}", e);
            actix_web::error::ErrorInternalServerError("Invalid image format")
        })?;

        let variants_to_create = vec![
            (
                400,
                "thumbnail",
                db_core::models::ImageResolution::Thumbnail400w,
            ),
            (720, "mobile", db_core::models::ImageResolution::Mobile720w),
            (
                1280,
                "tablet",
                db_core::models::ImageResolution::Tablet1280w,
            ),
            (
                1920,
                "desktop",
                db_core::models::ImageResolution::Desktop1920w,
            ),
            (
                2560,
                "highres",
                db_core::models::ImageResolution::HighRes2560w,
            ),
        ];

        let mut processed_variants = Vec::new();
        for (width, folder, resolution_enum) in variants_to_create {
            let resized = img.resize(width, u32::MAX, image::imageops::FilterType::Lanczos3);
            let webp_encoder =
                webp::Encoder::from_image(&resized).expect("Failed to create webp encoder");
            let encoded = webp_encoder.encode(80.0);
            let bytes = encoded.iter().copied().collect::<Vec<u8>>();
            processed_variants.push((width, folder, resolution_enum, bytes));
        }

        Ok(ImageTask {
            listing_id: self.listing_id,
            image_id: self.image_id,
            state: Processed(processed_variants),
        })
    }
}

impl ImageTask<Processed> {
    pub async fn upload(
        self,
        client: &Storage,
        public_bucket: &str,
        pool: &sqlx::PgPool,
        client_file_id: String,
        raw_url: String,
    ) -> Result<(), actix_web::Error> {
        let mut db_variants = Vec::new();
        let gcs_write_bucket = format!("projects/_/buckets/{}", public_bucket);

        for (_width, folder, resolution_enum, bytes) in self.state.0 {
            let target_name = format!(
                "optimized/{}/{}_{}.webp",
                folder, self.listing_id, self.image_id
            );

            if let Err(e) = client
                .write_object(
                    &gcs_write_bucket,
                    &target_name,
                    actix_web::web::Bytes::from(bytes.clone()),
                )
                .set_content_type("image/webp")
                .send_buffered()
                .await
            {
                tracing::error!("Failed to upload variant {}: {}", folder, e);
                continue;
            }

            let public_url = format!(
                "https://storage.googleapis.com/{}/{}",
                public_bucket, target_name
            );

            db_variants.push((
                self.listing_id,
                self.image_id, // parent_id
                client_file_id.clone(),
                resolution_enum,
                bytes.len() as i64,
                "image/webp".to_string(),
                public_url,
            ));
        }

        if let Err(e) = db_core::listing::insert_listing_image_variants(pool, &db_variants).await {
            tracing::error!("Failed to insert variants into DB: {}", e);
        }

        if let Err(e) =
            db_core::listing::mark_listing_image_processed(pool, self.image_id, raw_url).await
        {
            tracing::error!("Failed to mark image as processed in DB: {}", e);
        }

        Ok(())
    }
}

/// Processes an uploaded image triggered by a Google Cloud Pub/Sub notification.
///
/// This endpoint is called internally when a new image is uploaded to the raw GCS bucket.
/// It performs the following operations:
/// 1. Extracts the `listing_id` and `image_id` from the GCS object name (expected format: `listing_{id}/image_{id}`).
/// 2. Updates the image record in the database to a 'processing' state.
/// 3. Downloads the original image from the raw GCS bucket.
/// 4. Generates multiple optimized WebP variants of the image (thumbnail, mobile, tablet, desktop, highres).
/// 5. Uploads the transformed WebP variants to the public GCS bucket.
/// 6. Records the variants and their public URLs in the database.
/// 7. Marks the parent image record as fully processed.
///
/// # Arguments
///
/// * `req` - The Actix HTTP request.
/// * `payload` - A JSON body containing the Google Cloud Pub/Sub message.
/// * `pool` - The database connection pool.
///
/// # Returns
///
/// Returns a `200 OK` response on success, or an appropriate error on failure (e.g., 400 for invalid payload, 500 for database or GCS errors).
#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/internal/image/process_image",
    tag = "images",
    request_body = PubSubPayload,
    responses(
        (status = 200, description = "Image processed payload received"),
        (status = 400, description = "Bad Request"),
        (status = 500, description = "Internal Server Error")
    )
)]
pub async fn process_image(
    req: HttpRequest,
    payload: web::Json<PubSubPayload>,
    pool: web::Data<sqlx::PgPool>,
) -> Result<impl Responder, Error> {
    let payload_data = payload.into_inner();

    let object_metadata = match payload_data.message.decode_data() {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("Failed to decode payload data: {}", e);
            return Err(actix_web::error::ErrorBadRequest("Invalid payload data"));
        }
    };

    tracing::info!(
        "Received PubSub notification for object: {} (bucket: {}, {:?})",
        object_metadata.name,
        object_metadata.bucket,
        object_metadata.content_type
    );

    // 1. Extracts the `listing_id` and `image_id` from the GCS object name (expected format: `listing_{id}/image_{id}`).
    let parts: Vec<&str> = object_metadata.name.split('/').collect();
    if parts.len() < 2 {
        tracing::error!("Invalid object name format: {}", object_metadata.name);
        return Err(actix_web::error::ErrorBadRequest("Invalid object name"));
    }

    // example: "listing_1234/image_5678" -> "1234", "5678"
    let listing_id_str = parts[0].strip_prefix("listing_").unwrap_or(parts[0]);
    let image_id_str = parts[1].strip_prefix("image_").unwrap_or(parts[1]);

    let listing_id = match uuid::Uuid::parse_str(listing_id_str) {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Invalid listing ID in object name: {}", e);
            return Err(actix_web::error::ErrorBadRequest("Invalid listing ID"));
        }
    };

    let image_id = match uuid::Uuid::parse_str(image_id_str) {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Invalid image ID in object name: {}", e);
            return Err(actix_web::error::ErrorBadRequest("Invalid image ID"));
        }
    };

    let size_bytes: i64 = object_metadata
        ._size
        .as_deref()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    let content_type = object_metadata
        .content_type
        .clone()
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let raw_image_record = match db_core::listing::update_listing_image_to_processing(
        pool.get_ref(),
        image_id,
        size_bytes,
        content_type,
    )
    .await
    {
        Ok(r) => r,
        Err(db_core::error::DbError::Sqlx(sqlx::Error::RowNotFound)) => {
            tracing::warn!(
                "Image ID {} not found in database. This may be a stale Pub/Sub message for a deleted listing/image. Acknowledging message to stop retries.",
                image_id
            );
            return Ok(HttpResponse::Ok().finish());
        }
        Err(e) => {
            tracing::error!("Failed to update image to processing: {:?}", e);
            return Err(actix_web::error::ErrorInternalServerError("Database error"));
        }
    };

    // 3. Downloads the original image from the raw GCS bucket.
    use google_cloud_storage::client::Storage;

    let client = match Storage::builder().build().await {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Failed to configure GCS client auth: {}", e);
            return Err(actix_web::error::ErrorInternalServerError(
                "GCS config error",
            ));
        }
    };

    let task = ImageTask::new(listing_id, image_id);

    let downloaded = task
        .download(&client, &object_metadata.bucket, &object_metadata.name)
        .await?;
    let processed = downloaded.process()?;

    let public_bucket = std::env::var("GCS_PUBLIC_BUCKET")
        .unwrap_or_else(|_| "our-places-gcs-img-public".to_string());

    let raw_url = raw_image_record.upload_url.unwrap_or_else(|| {
        format!(
            "https://storage.googleapis.com/{}/{}",
            object_metadata.bucket, object_metadata.name
        )
    });

    processed
        .upload(
            &client,
            &public_bucket,
            pool.get_ref(),
            raw_image_record.client_file_id.clone(),
            raw_url,
        )
        .await?;

    Ok(HttpResponse::Ok().finish())
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            process_image,
            api_core::health::health_check,
        ),
        components(
            schemas(PubSubPayload, PubSubMessage, PubSubAttributes, api_core::health::PingResponse)
        ),
        tags(
            (name = "images", description = "Image processing endpoints")
        ),
    )]
    struct ApiDoc;

    // Register Swagger UI services at the ROOT scope so paths match
    cfg.service(
        SwaggerUi::new("/api/docs/swagger-ui/{_:.*}")
            .url("/api-docs/openapi.json", ApiDoc::openapi()),
    );

    cfg.service(
        web::scope("/api/v1/internal/image")
            .route(
                "/process_image",
                web::post()
                    .to(process_image)
                    .wrap(from_fn(content_negotiation_middleware)),
            )
            .route(
                "/health_check",
                web::get().to(api_core::health::health_check),
            ),
    );
}
