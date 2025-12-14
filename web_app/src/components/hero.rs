use leptos::*;
use leptos::prelude::*;
use crate::components::search_bar::SearchBar;

#[component]
pub fn Hero() -> impl IntoView {
    view! {
        <section class="hero relative w-full h-[600px] flex flex-col justify-center items-center text-center">
            // Background Image (Placeholder URL)
            <div class="absolute inset-0 z-0">
                <img 
                    src="https://images.unsplash.com/photo-1613490493576-7fde63acd811?q=80&w=2071&auto=format&fit=crop" 
                    alt="Luxury Villa" 
                    class="w-full h-full object-cover"
                />
                // Overlay for better text readability
                <div class="absolute inset-0 bg-black/20"></div>
            </div>

            // Content
            <div class="relative z-10 w-full px-4">
                <h1 class="text-5xl md:text-7xl font-bold text-white mb-4 drop-shadow-md">
                    "Find Your Perfect Getaway"
                </h1>
                <p class="text-xl md:text-2xl text-white mb-12 drop-shadow-md">
                    "Discover the world's most luxurious villas."
                </p>

                // Search Bar Component
                <SearchBar />
            </div>
        </section>
    }
}
