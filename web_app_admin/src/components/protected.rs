use leptos::children::ChildrenFn;
use leptos::prelude::*;
use leptos_router::components::Redirect;
use std::sync::{Arc, Mutex};

/// A component that protects its children by checking for authentication.
/// If not authenticated, redirects to /login.
#[component]
pub fn RequireAuth(children: ChildrenFn) -> impl IntoView {
    let children = Arc::new(Mutex::new(children));
    let auth_status = Resource::new(
        || (),
        |_| async move { crate::auth::get_current_user().await },
    );

    view! {
        <Suspense fallback=move || view! { <p>"Loading..."</p> }>
            {move || {
                match auth_status.get() {
                    Some(Ok(Some(_))) => {
                         let children = children.lock().unwrap();
                         (children)().into_view().into_any()
                    }
                    Some(Ok(None)) | Some(Err(_)) => {
                        view! { <Redirect path="/login"/> }.into_any()
                    }
                    None => view! { <p>"Loading authentication..."</p> }.into_any()
                }
            }}
        </Suspense>
    }
}
