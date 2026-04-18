#![recursion_limit = "512"]
#[cfg(feature = "ssr")]
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    use actix_files::Files;
    use actix_web::*;
    use leptos::config::get_configuration;
    use leptos::prelude::*;
    use leptos_actix::{generate_route_list, LeptosRoutes};
    use web_app::app::*;
    use web_app::components::shell::AppShell;

    use actix_session::{storage::CookieSessionStore, SessionMiddleware};
    use actix_web::cookie::Key;

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;

    use dotenvy::dotenv;

    dotenv().ok();
    tracing_subscriber::fmt::init();

    // For development, we'll use a hardcoded key.
    // In production, this MUST be an environment variable (64+ bytes).
    let secret_key = Key::from(
        "this-is-a-very-secret-and-at-least-64-bytes-long-key-for-development-purposes-only"
            .as_bytes(),
    );

    // Spawn background cleanup task
    tokio::spawn(async move {
        let pool = web_app_common::api_client::get_pool().await;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(600)); // every 10 mins
        loop {
            interval.tick().await;
            match db_core::booking::cleanup_stale_bookings(&pool, 120).await {
                Ok(count) => {
                    if count > 0 {
                        tracing::info!("Cleaned up {} stale bookings", count);
                    }
                }
                Err(e) => tracing::error!("Error cleaning up bookings: {:?}", e),
            }
        }
    });

    HttpServer::new(move || {
        // Generate the list of routes in your Leptos App
        let routes = generate_route_list(App);
        let leptos_options = &conf.leptos_options;
        let site_root = leptos_options.site_root.clone().to_string();

        println!("listening on http://{}", &addr);

        App::new()
            .wrap(SessionMiddleware::new(
                CookieSessionStore::default(),
                secret_key.clone(),
            ))
            // serve JS/WASM/CSS from `pkg`
            .service(Files::new("/pkg", format!("{site_root}/pkg")))
            // serve other assets from the `assets` directory
            .service(Files::new("/assets", &site_root))
            // serve the favicon from /favicon.ico
            .service(favicon)
            .leptos_routes(routes, {
                let leptos_options = leptos_options.clone();
                move || {
                    view! {
                        <AppShell options=leptos_options.clone()/>
                    }
                }
            })
            .app_data(web::Data::new(leptos_options.to_owned()))
        //.wrap(middleware::Compress::default())
    })
    .bind(&addr)?
    .run()
    .await
}

#[cfg(feature = "ssr")]
#[actix_web::get("favicon.ico")]
async fn favicon(
    leptos_options: actix_web::web::Data<leptos::config::LeptosOptions>,
) -> actix_web::Result<actix_files::NamedFile> {
    let leptos_options = leptos_options.into_inner();
    let site_root = &leptos_options.site_root;
    Ok(actix_files::NamedFile::open(format!(
        "{site_root}/favicon.ico"
    ))?)
}

#[cfg(not(any(feature = "ssr", feature = "csr")))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for pure client-side testing
    // see lib.rs for hydration function instead
    // see optional feature `csr` instead
}

#[cfg(all(not(feature = "ssr"), feature = "csr"))]
pub fn main() {
    // a client-side main function is required for using `trunk serve`
    // prefer using `cargo leptos serve` instead
    // to run: `trunk serve --open --features csr`
    use web_app::app::*;

    console_error_panic_hook::set_once();

    leptos::mount_to_body(App);
}
