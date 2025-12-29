use std::env;

fn main() {
    dotenvy::dotenv().ok();

    println!("cargo:rerun-if-changed=.env");

    if let Ok(val) = env::var("LISTING_API_URL") {
        println!("cargo:rustc-env=LISTING_API_URL={}", val);
    }
}
