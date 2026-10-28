use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::{View, view},
};

/// Placeholder landing page served until the redesigned pages land.
#[page("/")]
pub async fn home_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
        <head>
            <meta charset="utf-8" />
            <meta name="viewport" content="width=device-width, initial-scale=1" />
            <title>"Our Places"</title>
        </head>
        <body>
            <main id="app-root">"Our Places guest portal"</main>
        </body>
        </html>
    })
}
