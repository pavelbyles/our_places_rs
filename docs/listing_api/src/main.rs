//! API for OurPlaces
//!
//! ## Overview
//!
//! Provides a functionality to manage listings and reservations
//!
//! ## List of API's
//!
//! - [x] Create listings
//! - [ ] Get listings
//! - [ ] Update listings

// #[macro_use]
extern crate lazy_static;
use sqlx::PgPool;
use std::net::TcpListener;

use our_places_app_api_listing_rs::startup::run;

mod apis;
mod settings;
mod util;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    // Read logging config
    let result = log4rs::init_file("log4rs.yml", Default::default());
    match result {
        Ok(_) => {
            log::info!("Log config file loaded");
        }
        Err(error) => {
            println!("Could not load log4rs.yml: {}", error);
        }
    }

    // Get settings
    let config = settings::get_settings().expect("Could not load settings");

    // Create database connection pool
    let db_connection_pool = PgPool::connect(&config.database.connection_string())
        .await
        .expect("Failed to connect to database.");

    // Setup web server
    let http_port: u16 = util::sys::get_port(config.server.port);
    let address = format!("{}:{}", config.server.host, http_port);

    log::info!("Starting server on port: {}", http_port);

    println!("Env is: {}", config.env);
    println!("Port is: {}", http_port);

    let listener = TcpListener::bind(&address).expect("Failed to bind to random port");
    run(listener, db_connection_pool)?.await
}
