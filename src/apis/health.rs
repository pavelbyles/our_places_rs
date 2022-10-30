use actix_web::{get, HttpResponse, Responder};

#[get("/ping")]
async fn health_check() -> impl Responder {
    HttpResponse::Ok().body("alive")
}
