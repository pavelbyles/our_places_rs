use leptos::prelude::*;
use leptos_meta::{provide_meta_context, Stylesheet, Title};
use leptos_router::{
    components::{Outlet, ParentRoute, Route, Router, Routes},
    hooks::use_location, path,
};

use crate::auth::{get_current_user, UserProfile};
use crate::components::{
    about::AboutPage, home::HomePage, layout::Layout, layout_no_search::LayoutNoSearch,
    listing_detail::ListingDetailPage, listings::ListingsPage, login::LoginPage,
    not_found::NotFound, register::RegisterPage, verify::VerifyPage, checkout::CheckoutPage,
};

#[derive(Clone, Debug)]
pub struct AuthContext {
    pub user: Resource<Result<Option<UserProfile>, ServerFnError>>,
    refresh: RwSignal<u32>,
}

impl AuthContext {
    pub fn refresh(&self) {
        self.refresh.update(|n| *n += 1);
    }
}

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
            <AppContent/>
        </Router>
    }
}

#[component]
fn AppContent() -> impl IntoView {
    // Current user resource
    let refresh = RwSignal::new(0);
    let location = use_location();
    let user = Resource::new(
        move || (refresh.get(), location.pathname.get()),
        |_| async move { get_current_user().await }
    );
    provide_context(AuthContext { user, refresh });

    view! {
        <Routes fallback=move || "Not found.">
            <ParentRoute path=path!("") view=move || view! { <Layout><Outlet/></Layout> }>
                <Route path=path!("") view=HomePage/>
                <Route path=path!("home") view=HomePage/>
                <Route path=path!("about") view=AboutPage/>
                <Route path=path!("listing/:id") view=ListingDetailPage/>
                <Route path=path!("login") view=LoginPage/>
                <Route path=path!("register") view=RegisterPage/>
                <Route path=path!("verify") view=VerifyPage/>
                <Route path=path!("checkout/:id") view=CheckoutPage/>
            </ParentRoute>
            <ParentRoute path=path!("listings") view=move || view! { <LayoutNoSearch><Outlet/></LayoutNoSearch> }>
                <Route path=path!("") view=ListingsPage/>
            </ParentRoute>
            <Route path=path!("*any") view=NotFound/>
        </Routes>
    }
}
