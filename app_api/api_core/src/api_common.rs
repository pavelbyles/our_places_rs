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
