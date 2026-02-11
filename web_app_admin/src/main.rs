#[cfg(feature = "ssr")]
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    use actix_files::Files;
    use actix_session::config::PersistentSession;
    use actix_session::SessionMiddleware;
    use actix_web::cookie::{time::Duration, Key};
    use actix_web::*;
    use db_core::sessions::SessionsDb;
    use dotenvy::dotenv;
    use leptos::config::get_configuration;
    use leptos::prelude::*;
    use leptos_actix::{generate_route_list, LeptosRoutes};
    use sqlx::postgres::PgPoolOptions;
    use std::env;
    use web_app_admin::app::*;
    use web_app_admin::components::shell::AppShell;
    use web_app_admin::session_store::AdminSessionStore;

    dotenv().ok();
    tracing_subscriber::fmt::init();

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool: sqlx::PgPool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(60))
        .connect(&database_url)
        .await
        .expect("Failed to connect to database");

    let sessions_db = SessionsDb::new(pool.clone());
    let session_store = AdminSessionStore::new(sessions_db);
    let secret_key = Key::from(
        env::var("SESSION_SECRET")
            .unwrap_or_else(|_| {
                "0123456789012345678901234567890123456789012345678901234567890123".to_string()
            })
            .as_bytes(),
    );

    HttpServer::new(move || {
        // Generate the list of routes in your Leptos App
        let routes = generate_route_list(App);
        let leptos_options = &conf.leptos_options;
        let site_root = leptos_options.site_root.clone().to_string();

        println!("listening on http://{}", &addr);

        App::new()
            .wrap(
                SessionMiddleware::builder(session_store.clone(), secret_key.clone())
                    .cookie_secure(false) // For local dev
                    .session_lifecycle(PersistentSession::default().session_ttl(Duration::days(7)))
                    .build(),
            )
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
            .route(
                "/api/{tail:.*}",
                leptos_actix::handle_server_fns_with_context(move || {}),
            )
            .app_data(web::Data::new(leptos_options.to_owned()))
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
    use web_app_admin::app::*;

    console_error_panic_hook::set_once();

    leptos::mount_to_body(App);
}
