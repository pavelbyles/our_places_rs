use actix_web::http::header::ACCEPT;
use actix_web::{HttpRequest, HttpResponse};
use serde::Serialize;

pub enum Payload<T> {
    Item(T),
    Collection(Vec<T>),
}

/// Helper to determine response format
/// `wrap_xml` is a closure that takes a Vec<T> and returns a wrapper struct W that implements Serialize.
/// This is used because XML requires a custom root element for lists.
pub fn respond<T, W, F>(
    req: &HttpRequest,
    data: Payload<T>,
    wrap_xml: F,
    status: actix_web::http::StatusCode,
) -> HttpResponse
where
    T: Serialize,
    W: Serialize,
    F: FnOnce(Vec<T>) -> W,
{
    let accept_header = req.headers().get(ACCEPT);
    let use_xml =
        accept_header.is_some_and(|h| h.to_str().unwrap_or("").contains("application/xml"));

    if use_xml {
        let xml_body = match data {
            Payload::Item(item) => serde_xml_rs::to_string(&item),
            Payload::Collection(items) => {
                let wrapper = wrap_xml(items);
                serde_xml_rs::to_string(&wrapper)
            }
        };

        match xml_body {
            Ok(xml) => HttpResponse::build(status)
                .content_type("application/xml")
                .body(xml),
            Err(_) => HttpResponse::InternalServerError().finish(),
        }
    } else {
        match data {
            Payload::Item(item) => HttpResponse::build(status).json(item),
            Payload::Collection(items) => HttpResponse::build(status).json(items),
        }
    }
}
