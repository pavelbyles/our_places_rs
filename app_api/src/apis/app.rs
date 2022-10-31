use actix_web::{get, web, HttpResponse, Responder};

#[get("/hello")]
async fn greet_no_name() -> impl Responder {
    HttpResponse::Ok().body("Hello World!")
}

#[get("/hello/{name}")]
async fn greet_with_name(name: web::Path<String>) -> impl Responder {
    HttpResponse::Ok().body(format!("Hello {}!", name))
}
