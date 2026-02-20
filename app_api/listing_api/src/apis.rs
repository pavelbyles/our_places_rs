use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::http::header::{ACCEPT, CONTENT_TYPE};
use actix_web::middleware::{Next, from_fn};
use actix_web::{Error, HttpRequest, HttpResponse, Responder, web};
use api_core::models::{
    ListingsWrapper, map_listing_to_response, map_listing_with_owner_to_response,
};
use api_core::response::{Payload, respond};
use api_core::{error::ApiError, pagination, settings::Settings};
use common::models::{ListingQueryParams, ListingResponse};
use db_core::listing as db_listing;
use db_core::models::{NewListing, StructureType, UpdatedListing};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct NewListingRequest {
    #[schema(value_type = String, example = "Zen Loft")]
    #[validate(length(min = 1, message = "Name cannot be empty"))]
    pub name: String,

    #[schema(value_type = String, format = "uuid")]
    pub user_id: Uuid,

    #[serde(default)]
    #[schema(value_type = String, example = "A zen place to be")]
    #[validate(length(
        max = 2000,
        message = "Description cannot be longer than 2000 characters"
    ))]
    pub description: Option<String>,

    #[schema(value_type = String, example = "Apartment")]
    pub listing_structure: StructureType,

    #[serde(default)]
    #[schema(value_type = String, example = "Jamaica")]
    #[validate(length(min = 1, message = "Country cannot be empty"))]
    pub country: String,

    #[serde(default)]
    #[schema(value_type = String, example = "150.00")]
    pub price_per_night: Option<Decimal>,
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema)]
pub struct UpdatedListingRequest {
    #[serde(default)]
    #[validate(length(min = 1, message = "Name cannot be empty"))]
    pub name: Option<String>,

    #[serde(default)]
    #[validate(length(
        max = 2000,
        message = "Description cannot be longer than 2000 characters"
    ))]
    pub description: Option<String>,

    #[serde(default)]
    #[schema(value_type = Option<String>, example = "Villa")]
    pub listing_structure: Option<StructureType>,

    #[serde(default)]
    #[validate(length(min = 1, message = "Country cannot be empty"))]
    pub country: Option<String>,

    #[serde(default)]
    #[schema(value_type = Option<String>, example = "150.00")]
    pub price_per_night: Option<Decimal>,

    #[serde(default)]
    pub is_active: Option<bool>,
}

/// Gives first 10 listings if no page or per_page is provided
#[tracing::instrument]
#[utoipa::path(
    get,
    path = "/api/v1/listings",
    tag = "listings",
    params(
        ListingQueryParams
    ),
    responses(
        (status = 200, description = "List of listings", body = [ListingResponse]),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn get_listings(
    pool: web::Data<sqlx::PgPool>,
    query: web::Query<common::models::ListingQueryParams>,
    req: actix_web::HttpRequest,
) -> Result<HttpResponse, actix_web::Error> {
    let mut structure_types = Vec::new();
    let qs = req.query_string();

    if let Ok(params) = serde_urlencoded::from_str::<Vec<(String, String)>>(qs) {
        for (key, value) in params {
            if key == "structure_type" {
                for part in value.split(',') {
                    let trimmed = part.trim();
                    if !trimmed.is_empty() {
                        structure_types.push(trimmed.to_string());
                    }
                }
            }
        }
    }

    // Set default values for pagination if they are not provided.
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10);

    // Minimum of per_page or 100
    let per_page_clamped = per_page.min(100);

    let filter = common::models::ListingFilter {
        name: query.name.clone(),
        country: query.country.clone(),
        min_price: query.min_price,
        max_price: query.max_price,
        structure_type: structure_types,
        owner: query.owner.clone(),
    };

    let listings = db_listing::get_listings(pool.get_ref(), page, per_page_clamped, Some(filter))
        .await
        .map_err(ApiError::Database)?;
    let response: Vec<ListingResponse> = listings
        .into_iter()
        .map(map_listing_with_owner_to_response)
        .collect();

    Ok(respond(
        &req,
        Payload::Collection(response),
        |items| ListingsWrapper { listing: items },
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument]
#[utoipa::path(
    get,
    path = "/api/v1/listings/listing/{id}",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID")
    ),
    responses(
        (status = 200, description = "Listing found", body = ListingResponse),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_listing_by_id(
    req: HttpRequest,
    path: web::Path<Uuid>,
    pool: web::Data<PgPool>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    let listing = db_listing::get_listing_by_id(pool.get_ref(), listing_id).await?;

    Ok(respond(
        &req,
        Payload::Item(map_listing_to_response(listing)),
        |_: Vec<ListingResponse>| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[tracing::instrument]
#[utoipa::path(
    post,
    path = "/api/v1/listings/listing",
    tag = "listings",
    request_body = NewListingRequest,
    responses(
        (status = 201, description = "Listing created", body = ListingResponse),
        (status = 400, description = "Validation error"),
        (status = 500, description = "Internal server error")
    )
)]
async fn create_listing(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    new_listing: web::Json<NewListingRequest>,
    settings: web::Data<Settings>,
) -> Result<impl Responder, ApiError> {
    let req_data = new_listing.into_inner();
    req_data.validate()?;

    let structure_id = req_data.listing_structure.id();

    let mut attempts = 0;
    let max_attempts = settings.application.max_attempts;

    loop {
        attempts += 1;
        let listing = NewListing {
            name: req_data.name.clone(),
            user_id: req_data.user_id,
            description: req_data.description.clone(),
            listing_structure_id: structure_id,
            country: req_data.country.clone(),
            price_per_night: req_data.price_per_night,
        };

        match db_listing::create_listing(pool.get_ref(), &listing).await {
            Ok(created_listing) => {
                return Ok(respond(
                    &req,
                    Payload::Item(map_listing_to_response(created_listing)),
                    |_: Vec<ListingResponse>| (),
                    actix_web::http::StatusCode::CREATED,
                ));
            }
            Err(e) => {
                match e {
                    db_core::error::DbError::ValidationError(msg) => {
                        let mut map = std::collections::HashMap::new();
                        map.insert(
                            std::borrow::Cow::from("validation"),
                            validator::ValidationErrorsKind::Field(vec![
                                validator::ValidationError::new("custom").with_message(msg.into()),
                            ]),
                        );
                        return Err(ApiError::ValidationError(validator::ValidationErrors(map)));
                    }
                    db_core::error::DbError::Sqlx(ref sqlx_error) => {
                        if let Some(db_error) = sqlx_error.as_database_error()
                            && db_error.code().as_deref() == Some("23505")
                            && let Some(constraint) = db_error.constraint()
                            && constraint == "listing_pkey"
                        {
                            if attempts >= max_attempts {
                                return Err(ApiError::Internal);
                            }
                            continue;
                        }
                    }
                }
                return Err(ApiError::Database(e));
            }
        }
    }
}

#[tracing::instrument]
#[utoipa::path(
    patch,
    path = "/api/v1/listings/listing/{id}",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID")
    ),
    request_body = UpdatedListingRequest,
    responses(
        (status = 200, description = "Listing updated", body = ListingResponse),
        (status = 400, description = "Validation error"),
        (status = 500, description = "Internal server error")
    )
)]
async fn update_listing(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    updated_listing_req: web::Json<UpdatedListingRequest>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    let req_data = updated_listing_req.into_inner();
    req_data.validate()?;

    let structure_id = req_data.listing_structure.map(|s| s.id());

    let updated_data = UpdatedListing {
        name: req_data.name,
        description: req_data.description,
        listing_structure_id: structure_id,
        country: req_data.country,
        price_per_night: req_data.price_per_night,
        is_active: req_data.is_active,
    };

    let updated_listing =
        db_listing::update_listing(pool.get_ref(), listing_id, &updated_data).await?;

    Ok(respond(
        &req,
        Payload::Item(map_listing_to_response(updated_listing)),
        |_: Vec<ListingResponse>| (),
        actix_web::http::StatusCode::OK,
    ))
}

#[derive(Deserialize, IntoParams, Debug)]
pub struct DeleteListingParams {
    pub hard_delete: Option<bool>,
}

#[tracing::instrument]
#[utoipa::path(
    delete,
    path = "/api/v1/listings/isting/{id}",
    tag = "listings",
    params(
        ("id" = String, Path, description = "Listing UUID"),
        DeleteListingParams
    ),
    responses(
        (status = 204, description = "Listing deleted"),
        (status = 404, description = "Listing not found"),
        (status = 500, description = "Internal server error")
    )
)]
async fn delete_listing(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    query: web::Query<DeleteListingParams>,
    settings: web::Data<Settings>,
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    let hard_delete = query.hard_delete.unwrap_or(false);

    // Guard: Prevent hard delete if the feature flag is off
    if hard_delete && !settings.feature_flags.enable_hard_deletes {
        return Err(ApiError::FeatureDisabled("hard_delete".to_string()));
    }

    db_listing::delete_listing(pool.get_ref(), listing_id, hard_delete).await?;

    Ok(HttpResponse::NoContent().finish())
}

pub fn configure_routes(cfg: &mut web::ServiceConfig) {
    #[derive(OpenApi)]
    #[openapi(
        paths(
            get_listings,
            create_listing,
            get_listing_by_id,
            update_listing,
            delete_listing,
            api_core::health::health_check,
        ),
        components(
            schemas(NewListingRequest, UpdatedListingRequest, ListingResponse, pagination::Pagination, common::models::ListingFilter)
        ),
        tags(
            (name = "listings", description = "Listing management endpoints")
        ),
    )]
    struct ApiDoc;

    // Register Swagger UI services at the ROOT scope so paths match
    cfg.service(
        SwaggerUi::new("/api/docs/swagger-ui/{_:.*}")
            .url("/api-docs/openapi.json", ApiDoc::openapi()),
    );

    cfg.service(
        web::scope("/api/v1/listings")
            .route(
                "/health_check",
                web::get().to(api_core::health::health_check),
            )
            .route(
                "/",
                web::get()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(get_listings),
            )
            .route(
                "/",
                web::post()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(create_listing),
            )
            .route(
                "/listing/{id}",
                web::get()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(get_listing_by_id),
            )
            .route(
                "/listing/{id}",
                web::patch()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(update_listing),
            )
            .route(
                "/listing/{id}",
                web::delete()
                    .wrap(from_fn(content_negotiation_middleware))
                    .to(delete_listing),
            ),
    );
}

/// Content-Type - Requests
/// Accept - Responses
/// Middleware to check Content-Type and Accept headers
/// Returns 415 Unsupported Media Type or 406 Not Acceptable if invalid
async fn content_negotiation_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let headers = req.headers();

    // Check Content-Type (if present) -> 415 Unsupported Media Type
    if let Some(ct_str) = headers.get(CONTENT_TYPE).and_then(|ct| ct.to_str().ok()) {
        let mime = ct_str.split(';').next().unwrap_or("").trim().to_lowercase();
        let supported_formats = [
            "application/json",
            "application/xml",
            "application/x-www-form-urlencoded",
        ];

        if !supported_formats.contains(&mime.as_str()) {
            return Err(actix_web::error::ErrorUnsupportedMediaType(
                "Unsupported Content-Type",
            ));
        }
    }

    // Check Accept header (if present) -> 406 Not Acceptable
    if let Some(accept_str) = headers.get(ACCEPT).and_then(|a| a.to_str().ok()) {
        let supported_responses = ["application/json", "application/xml"];

        let accepts_supported = accept_str.split(',').any(|s| {
            let mime = s.split(';').next().unwrap_or("").trim().to_lowercase();
            mime == "*/*" || supported_responses.contains(&mime.as_str())
        });

        if !accepts_supported {
            return Err(actix_web::error::ErrorNotAcceptable(
                "The requested response format is not supported",
            ));
        }
    }

    // If checks pass, call the next service in the chain
    next.call(req).await
}

#[cfg(test)]
#[path = "apis_test.rs"]
mod tests;
