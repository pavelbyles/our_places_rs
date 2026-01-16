use leptos::prelude::*;
use leptos_meta::Title;

#[component]
pub fn AboutPage() -> impl IntoView {
    view! {
        <>
            <Title text="About Us" />
            <div class="p-4">
                <h1 class="text-2xl font-bold mb-4">"About Us"</h1>
                <p class="mb-4">"Welcome to Our Places! We are dedicated to finding you the best places to stay."</p>
                <button class="btn btn-primary">"Contact Us"</button>
            </div>
        </>
    }
}
