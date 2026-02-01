use leptos::prelude::*;

#[component]
pub fn Hero() -> impl IntoView {
    let auth_status = Resource::new(
        || (),
        |_| async move { crate::auth::get_current_user().await },
    );

    view! {
        <div
            class="hero min-h-screen"
            style="background-image: url(https://img.daisyui.com/images/stock/photo-1507358522600-9f71e620c44e.webp);"
            >
            <div class="hero-overlay"></div>
            <div class="hero-content text-neutral-content text-center">
                <div class="max-w-md">
                <h1 class="mb-5 text-5xl font-bold">"Your home away from home"</h1>
                <p class="mb-5">
                    "Provident cupiditate voluptatem et in. Quaerat fugiat ut assumenda excepturi exercitationem
                    quasi. In deleniti eaque aut repudiandae et a id nisi."
                </p>
                <Transition
                    fallback=move || view! { <a href="/login" class="btn btn-primary">"Login"</a> }
                >
                    {move || {
                        auth_status.get().map(|status| {
                            match status {
                                Ok(Some(name)) => {
                                    let logout_action = ServerAction::<crate::auth::Logout>::new();
                                    view! {
                                        <ActionForm action=logout_action>
                                            <button type="submit" class="btn btn-primary">{format!("Sign Out ({})", name)}</button>
                                        </ActionForm>
                                    }.into_any()
                                },
                                _ => view! { <a href="/login" class="btn btn-primary">"Login"</a> }.into_any(),
                            }
                        })
                    }}
                </Transition>
                </div>
            </div>
        </div>
    }
}
