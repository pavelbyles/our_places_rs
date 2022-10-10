#![deny(warnings)]
extern crate hyper;
// extern crate pretty_env_logger;

use hyper::rt::{self, Future};
use hyper::service::service_fn_ok;
use hyper::{Body, Response, Server};
//use log::{error, info, warn};
//use log4rs::append::console::ConsoleAppender;
//use log4rs::append::file::FileAppender;
//use log4rs::config::{Appender, Config, Logger, Root};
//use log4rs::encode::pattern::PatternEncoder;
use std::env;

fn main() {
    // pretty_env_logger::init();

    println!(
        "CWD is: {}",
        env::current_dir()
            .unwrap()
            .into_os_string()
            .into_string()
            .unwrap()
    );

    /*    log4rs::init_file("log4rs.yml", Default::default()).unwrap();
    info!("Info log");
    warn!("Warn log");
    error!("Error log");*/

    let mut port: u16 = 8080;
    match env::var("PORT") {
        Ok(p) => {
            match p.parse::<u16>() {
                Ok(n) => {
                    let sum: i32 = add(1, 1);
                    println!("PORT env variable found");
                    println!(
                        "Using port set from environment variable PORT: {}, {}",
                        n, sum
                    );
                    port = n;
                }
                Err(_e) => {}
            };
        }
        Err(_e) => {}
    };
    let addr = ([0, 0, 0, 0], port).into();

    let new_service = || {
        service_fn_ok(|_| {
            let mut res_body = "Hello ".to_string();
            match env::var("TARGET") {
                Ok(target) => {
                    res_body.push_str(&target);
                }
                Err(_e) => res_body.push_str("World"),
            };

            Response::new(Body::from(res_body))
        })
    };

    let server = Server::bind(&addr)
        .serve(new_service)
        .map_err(|e| eprintln!("Server error: {}", e));

    println!("Listening on http://{}", addr);

    rt::run(server);
}

fn add(x: i32, y: i32) -> i32 {
    return x + y;
}

#[cfg(test)]
mod basic_tests {

    use super::*;
    #[test]
    fn add_test() {
        let expected_result = 2 + 2;
        assert_eq!(add(2, 2), expected_result);
    }
}
