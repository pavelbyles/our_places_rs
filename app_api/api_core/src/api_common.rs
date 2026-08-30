use actix_web::Error;
use actix_web::body::MessageBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::http::header::{ACCEPT, CONTENT_TYPE};
use actix_web::middleware::Next;

/// Content-Type - Requests
/// Accept - Responses
/// Middleware to check Content-Type and Accept headers
/// Returns 415 Unsupported Media Type or 406 Not Acceptable if invalid
pub async fn content_negotiation_middleware(
    req: ServiceRequest,
    next: Next<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    let headers = req.headers();

    // Validate Content-Type (if present) -> 415 Unsupported Media Type
    const SUPPORTED_CONTENT_TYPES: &[&str] = &[
        "application/json",
        "application/xml",
        "application/x-www-form-urlencoded",
    ];

    if let Some(false) = headers
        .get(CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .map(|ct_str| {
            let mime = ct_str.split(';').next().unwrap_or("").trim().to_lowercase();
            SUPPORTED_CONTENT_TYPES.contains(&mime.as_str())
        })
    {
        return Err(actix_web::error::ErrorUnsupportedMediaType(
            "Unsupported Content-Type",
        ));
    }

    // Validate Accept header (if present) -> 406 Not Acceptable
    const SUPPORTED_ACCEPT_TYPES: &[&str] = &["application/json", "application/xml"];

    if let Some(false) = headers
        .get(ACCEPT)
        .and_then(|a| a.to_str().ok())
        .map(|accept_str| {
            accept_str.split(',').any(|s| {
                let mime = s.split(';').next().unwrap_or("").trim().to_lowercase();
                mime == "*/*" || SUPPORTED_ACCEPT_TYPES.contains(&mime.as_str())
            })
        })
    {
        return Err(actix_web::error::ErrorNotAcceptable(
            "The requested response format is not supported",
        ));
    }

    // If checks pass, call the next service in the chain
    next.call(req).await
}
