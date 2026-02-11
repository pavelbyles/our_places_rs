use crate::components::hero::Hero;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_meta::Title;

#[component]
#[allow(non_snake_case)]
pub fn HomePage() -> impl IntoView {
    // Creates a reactive value to update the button
    let count = RwSignal::new(0);
    let on_click = move |_| {
        spawn_local(async move {
            let new_count = update_count(count.get()).await.unwrap_or(0);
            count.set(new_count);
        });
    };

    view! {
        <>
            <Title text="Home" />
            <Hero />
            <h1>"Welcome to Leptos!"</h1>
            <button class="btn btn-primary" on:click=on_click>"Click Me: " {move || count.get()}</button>
        </>
    }
}

#[server]
pub async fn update_count(count: i32) -> Result<i32, ServerFnError> {
    Ok(count + 1)
}
