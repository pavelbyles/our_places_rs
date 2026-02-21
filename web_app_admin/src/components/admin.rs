use crate::components::protected::RequireAuth;
use leptos::prelude::*;

#[component]
pub fn AdminPage() -> impl IntoView {
    view! {
        <RequireAuth>
            <h1>"Admin Page"</h1>
        </RequireAuth>

    }
}
