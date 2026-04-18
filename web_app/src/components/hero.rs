use leptos::prelude::*;

#[component]
pub fn Hero() -> impl IntoView {
    view! {
        <div
            class="hero min-h-screen"
            style="background-image: url(assets/hero_image.jpg);"
            >
            <div class="hero-overlay"></div>
            <div class="hero-content text-neutral-content text-center">
                <div class="max-w-md">
                <h1 class="mb-5 text-5xl font-bold">"Your home away from home"</h1>
                <p class="mb-5">
                    "Discover our handpicked collection of exceptional stays. From a luxury beachfront villa to highly-rated city apartments, step into an experience that is truly one-of-a-kind."
                </p>
                <button class="btn btn-primary">"Book Now"</button>
                </div>
            </div>
        </div>
    }
}
