use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::http::header::{ACCEPT, CONTENT_TYPE};
use actix_web::middleware::{Next, from_fn};
use actix_web::{Error, HttpRequest, HttpResponse, Responder, web};
use api_core::models::{ListingResponse, ListingsWrapper, map_listing_to_response};

// Helper to map StructureType to ID
fn structure_type_to_id(st: &StructureType) -> i32 {
    match st {
        StructureType::Apartment => 1,
        StructureType::House => 2,
        StructureType::Townhouse => 3,
        StructureType::Studio => 4,
        StructureType::Villa => 5,
    }
}
use api_core::response::{Payload, respond};
use api_core::{error::ApiError, pagination, settings::Settings};
// use chrono::{DateTime, Utc}; // Removed unused
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

#[derive(Debug, Deserialize, Validate, ToSchema)]
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

// Wrapper for XML collections is now in api_core

/// Gives first 10 listings if no page or per_page is provided
#[tracing::instrument]
#[utoipa::path(
    get,
    path = "/api/v1/listings",
    tag = "listings",
    params(
        pagination::Pagination
    ),
    responses(
        (status = 200, description = "List of listings", body = [ListingResponse]),
        (status = 500, description = "Internal server error")
    )
)]
async fn get_listings(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    query: web::Query<pagination::Pagination>,
) -> Result<impl Responder, ApiError> {
    // Set default values for pagination if they are not provided.
    let page = query.page.unwrap_or(1);
    let per_page = query.per_page.unwrap_or(10);

    // Minimum of per_page or 100
    let per_page_clamped = per_page.min(100);

    let listings = db_listing::get_listings(pool.get_ref(), page, per_page_clamped).await?;
    let response: Vec<ListingResponse> =
        listings.into_iter().map(map_listing_to_response).collect();

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

    let structure_id = match req_data.listing_structure {
        StructureType::Apartment => 1,
        StructureType::House => 2,
        StructureType::Townhouse => 3,
        StructureType::Studio => 4,
        StructureType::Villa => 5,
    };

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
                let db_core::error::DbError::Sqlx(ref sqlx_error) = e;
                if let Some(db_error) = sqlx_error.as_database_error() {
                    if db_error.code().as_deref() == Some("23505") {
                        if let Some(constraint) = db_error.constraint() {
                            if constraint == "listing_pkey" {
                                if attempts >= max_attempts {
                                    return Err(ApiError::Internal);
                                }
                                continue;
                            }
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

    let structure_id = req_data.listing_structure.map(|s| structure_type_to_id(&s));

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
) -> Result<impl Responder, ApiError> {
    let listing_id = path.into_inner();
    let hard_delete = query.hard_delete.unwrap_or(false);

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
            delete_listing
        ),
        components(
            schemas(NewListingRequest, UpdatedListingRequest, ListingResponse, pagination::Pagination)
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
mod tests {
    use super::*;
    use actix_web::{App, test, web};
    use chrono::Utc;
    use db_core::models::{NewListing, StructureType};
    use rust_decimal_macros::dec;
    use sqlx_db_tester::TestPg;
    use std::env;
    use std::path::Path;

    #[actix_web::test]
    async fn test_get_listing_by_id_success() {
        dotenvy::dotenv().ok();
        let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let migrations_path = Path::new("../../db_core/migrations");
        let test_db = TestPg::new(db_url, migrations_path);
        let pool = test_db.get_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let new_listing = NewListing {
            name: "API Test Listing".to_string(),
            user_id: Uuid::new_v4(),
            description: None,
            listing_structure_id: 1,
            country: "Testland".to_string(),
            price_per_night: Some(dec!(100.00)),
        };
        let created_listing = db_listing::create_listing(&mut *conn, &new_listing)
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .configure(configure_routes),
        )
        .await;

        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/listings/listing/{}", created_listing.id))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert!(resp.status().is_success());
        let body: ListingResponse = test::read_body_json(resp).await;
        assert_eq!(body.id, created_listing.id);
        assert_eq!(body.listing_structure, StructureType::Apartment);
    }

    #[actix_web::test]
    async fn test_get_listing_by_id_not_found() {
        dotenvy::dotenv().ok();
        let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let migrations_path = Path::new("../../db_core/migrations");

        let test_db = TestPg::new(db_url, migrations_path);
        let pool = test_db.get_pool().await;

        let non_existent_id = Uuid::new_v4();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .configure(configure_routes),
        )
        .await;

        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/listings/listing/{}", non_existent_id))
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn test_create_listing_validation_error() {
        dotenvy::dotenv().ok();
        let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let migrations_path = Path::new("../../db_core/migrations");
        let test_db = TestPg::new(db_url, migrations_path);
        let pool = test_db.get_pool().await;

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .configure(configure_routes),
        )
        .await;

        // Create a listing with an EMPTY name
        let invalid_listing = NewListingRequest {
            name: "".to_string(), // invalid name - should not be empty
            user_id: Uuid::new_v4(),
            description: None,
            listing_structure: StructureType::Apartment,
            country: "Testland".to_string(),
            price_per_night: Some(dec!(100.00)),
        };

        let req = test::TestRequest::post()
            .uri("/api/v1/listing/listings")
            .set_json(&invalid_listing)
            .to_request();
        let resp = test::call_service(&app, req).await;

        assert_eq!(resp.status(), 400);
    }

    #[actix_web::test]
    async fn test_delete_listing() {
        dotenvy::dotenv().ok();
        let db_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let migrations_path = Path::new("../../db_core/migrations");
        let test_db = TestPg::new(db_url, migrations_path);
        let pool = test_db.get_pool().await;
        let mut conn = pool.acquire().await.unwrap();

        let new_listing = NewListing {
            name: "Delete Me Listing".to_string(),
            user_id: Uuid::new_v4(),
            description: None,
            listing_structure_id: 1,
            country: "Testland".to_string(),
            price_per_night: Some(dec!(100.00)),
        };
        let created_listing = db_listing::create_listing(&mut *conn, &new_listing)
            .await
            .unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(pool.clone()))
                .configure(configure_routes),
        )
        .await;

        // 1. Soft Delete
        let req = test::TestRequest::delete()
            .uri(&format!("/api/v1/listings/listing/{}", created_listing.id))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 204);

        // Verify it's gone from GET
        let req = test::TestRequest::get()
            .uri(&format!("/api/v1/listingslisting/{}", created_listing.id))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);

        // 2. Hard Delete (on already soft-deleted item? Or create new one?)
        // Let's create another one for hard delete
        let new_listing_2 = NewListing {
            name: "Hard Delete Me".to_string(),
            user_id: Uuid::new_v4(),
            description: None,
            listing_structure_id: 1,
            country: "Testland".to_string(),
            price_per_night: Some(dec!(100.00)),
        };
        let created_listing_2 = db_listing::create_listing(&mut *conn, &new_listing_2)
            .await
            .unwrap();

        let req = test::TestRequest::delete()
            .uri(&format!(
                "/api/v1/listings/listing/{}?hard_delete=true",
                created_listing_2.id
            ))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 204);

        // Verify it's gone
        let req = test::TestRequest::get()
            .uri(&format!(
                "/api/v1/listings/listing/{}",
                created_listing_2.id
            ))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
    }

    #[actix_web::test]
    async fn test_xml_serialization_of_vec() {
        use db_core::models::StructureType;
        use rust_decimal_macros::dec;
        use serde_xml_rs::to_string;

        let response = vec![ListingResponse {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            name: "Test".to_string(),
            description: None,
            listing_structure: StructureType::Apartment,
            country: "Test".to_string(),
            price_per_night: Some(dec!(100)),
            is_active: true,
            added_at: Utc::now(),
        }];

        let wrapper = ListingsWrapper { listing: response };
        let xml = to_string(&wrapper);
        assert!(xml.is_ok(), "XML serialization failed: {:?}", xml.err());
    }
}
