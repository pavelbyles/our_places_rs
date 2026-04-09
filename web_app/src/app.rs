use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Stylesheet, Title};
use leptos_router::{
    components::{Outlet, ParentRoute, Route, Router, Routes},
    path,
};

use crate::components::{
    about::AboutPage, home::HomePage, layout::Layout, layout_no_search::LayoutNoSearch,
    listing_detail::ListingDetailPage, listings::ListingsPage, not_found::NotFound,
};

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/web_app.css"/>


        // sets the document title
        <Title formatter=|text| format!("{text} - Our Places")/>

        // content for this welcome page
        <Router>
            <Routes fallback=move || "Not found.">
                <ParentRoute path=path!("") view=move || view! { <Layout><Outlet/></Layout> }>
                    <Route path=path!("") view=HomePage/>
                    <Route path=path!("home") view=HomePage/>
                    <Route path=path!("about") view=AboutPage/>
                    <Route path=path!("listing/:id") view=ListingDetailPage/>
                </ParentRoute>
                <ParentRoute path=path!("listings") view=move || view! { <LayoutNoSearch><Outlet/></LayoutNoSearch> }>
                    <Route path=path!("") view=ListingsPage/>
                </ParentRoute>
                <Route path=path!("*any") view=NotFound/>
            </Routes>
        </Router>
    }
}
