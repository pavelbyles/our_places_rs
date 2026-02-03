use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment, WildcardSegment,
};

use crate::components::{
    about::AboutPage, admin::AdminPage, home::HomePage, layout::Layout, login::LoginPage,
    not_found::NotFound, user::UsersPage,
};

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/web_app_admin.css"/>


        // sets the document title
        <Title formatter=|text| format!("{text} - Our Places")/>

        // content for this welcome page
        <Router>
            <Layout>
                <main>
                    <Routes fallback=move || "Not found.">
                        <Route path=StaticSegment("") view=HomePage/>
                        <Route path=StaticSegment("/home") view=HomePage/>
                        <Route path=StaticSegment("/login") view=LoginPage/>
                        <Route path=StaticSegment("/about") view=AboutPage/>
                        <Route path=StaticSegment("/admin") view=AdminPage/>
                        <Route path=(StaticSegment("admin"), StaticSegment("users")) view=UsersPage/>
                        <Route path=WildcardSegment("any") view=NotFound/>
                    </Routes>
                </main>
            </Layout>
        </Router>
    }
}
