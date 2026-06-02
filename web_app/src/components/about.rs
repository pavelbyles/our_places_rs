use leptos::prelude::*;
use leptos_meta::Title;

#[component]
pub fn AboutPage() -> impl IntoView {
    view! {
        <>
            <Title text="About Us" />

            // Hero / Header Section
            <div class="hero bg-base-200 py-16 px-4">
                <div class="hero-content text-center max-w-3xl flex flex-col gap-6">
                    <span class="text-primary font-semibold tracking-wider uppercase text-sm">"Our Mission"</span>
                    <h1 class="text-5xl font-extrabold tracking-tight">"Connecting People to Extraordinary Places"</h1>
                    <p class="text-lg text-base-content/75 leading-relaxed">
                        "At Our Places, we believe travel is about more than just visiting new locations—it is about feeling at home wherever you go. We curate a handpicked collection of the world's most unique, comfortable, and beautifully designed properties to provide you with unforgettable stays."
                    </p>
                </div>
            </div>

            // Core Values / Grid Section
            <div class="py-16 max-w-6xl mx-auto px-4 flex flex-col gap-12">
                <div class="text-center max-w-2xl mx-auto flex flex-col gap-2">
                    <h2 class="text-3xl font-bold tracking-tight">"Why Choose Us"</h2>
                    <p class="text-base-content/60">"The core pillars of our commitment to host and traveler satisfaction."</p>
                </div>

                <div class="grid grid-cols-1 md:grid-cols-3 gap-8">
                    // Card 1
                    <div class="card bg-base-100 border border-base-200 shadow-sm hover:shadow-md transition-shadow">
                        <div class="card-body gap-4">
                            <div class="w-12 h-12 rounded-full bg-primary/10 flex items-center justify-center text-primary">
                                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-6 h-6">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M9 12.75 11.25 15 15 9.75M21 12c0 1.268-.63 2.39-1.593 3.068a3.745 3.745 0 0 1-1.043 3.296 3.745 3.745 0 0 1-3.296 1.043A3.745 3.745 0 0 1 12 21c-1.268 0-2.39-.63-3.068-1.593a3.746 3.746 0 0 1-3.296-1.043 3.745 3.745 0 0 1-1.043-3.296A3.745 3.745 0 0 1 3 12c0-1.268.63-2.39 1.593-3.068a3.746 3.746 0 0 1 1.043-3.296 3.746 3.746 0 0 1 3.296-1.043A3.746 3.746 0 0 1 12 3c1.268 0 2.39.63 3.068 1.593a3.746 3.746 0 0 1 3.296 1.043 3.746 3.746 0 0 1 1.043 3.296A3.745 3.745 0 0 1 21 12Z" />
                                </svg>
                            </div>
                            <h3 class="card-title text-xl font-bold">"Curated Quality"</h3>
                            <p class="text-base-content/70">"Every property undergoes a rigorous screening process to ensure exceptional standards, style, and hospitality."</p>
                        </div>
                    </div>

                    // Card 2
                    <div class="card bg-base-100 border border-base-200 shadow-sm hover:shadow-md transition-shadow">
                        <div class="card-body gap-4">
                            <div class="w-12 h-12 rounded-full bg-secondary/10 flex items-center justify-center text-secondary">
                                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-6 h-6">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M16.5 10.5V6.75a4.5 4.5 0 1 0-9 0v3.75m-.75 11.25h10.5a2.25 2.25 0 0 0 2.25-2.25v-6.75a2.25 2.25 0 0 0-2.25-2.25H6.75a2.25 2.25 0 0 0-2.25 2.25v6.75a2.25 2.25 0 0 0 2.25 2.25Z" />
                                </svg>
                            </div>
                            <h3 class="card-title text-xl font-bold">"Secure Booking"</h3>
                            <p class="text-base-content/70">"Book with peace of mind. Our state-of-the-art encryption and instant validation protect all transactions."</p>
                        </div>
                    </div>

                    // Card 3
                    <div class="card bg-base-100 border border-base-200 shadow-sm hover:shadow-md transition-shadow">
                        <div class="card-body gap-4">
                            <div class="w-12 h-12 rounded-full bg-accent/10 flex items-center justify-center text-accent">
                                <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="2" stroke="currentColor" class="w-6 h-6">
                                    <path stroke-linecap="round" stroke-linejoin="round" d="M6.633 10.25c.896 0 1.7-.393 2.287-1.096A3.377 3.377 0 0 1 12 7.75c.9 0 1.7.393 2.287 1.096.586.703 1.39 1.096 2.287 1.096.393 0 .786-.046 1.175-.138m-.9-2.25h.008v.008h-.008V6.75Zm-1.8 0h.008v.008h-.008V6.75Zm-1.8 0h.008v.008h-.008V6.75Zm-1.8 0h.008v.008h-.008V6.75Zm-1.8 0h.008v.008h-.008V6.75Zm-1.8 0h.008v.008h-.008V6.75Zm-1.8 0h.008v.008h-.008V6.75ZM2.25 12l8.954-8.955c.44-.439 1.152-.439 1.591 0L21.75 12M4.5 9.75v10.125c0 .621.504 1.125 1.125 1.125H9.75v-4.875c0-.621.504-1.125 1.125-1.125h2.25c.621 0 1.125.504 1.125 1.125V21h4.125c.621 0 1.125-.504 1.125-1.125V9.75M8.25 21h8.25" />
                                </svg>
                            </div>
                            <h3 class="card-title text-xl font-bold">"True Hospitality"</h3>
                            <p class="text-base-content/70">"Experience locally rooted stays hosted by real, passionate individuals who love sharing their space."</p>
                        </div>
                    </div>
                </div>
            </div>

            // Stats Section
            <div class="bg-base-200 py-16 px-4">
                <div class="max-w-6xl mx-auto flex flex-col items-center gap-8">
                    <div class="stats stats-vertical lg:stats-horizontal shadow-lg w-full bg-base-100 border border-base-300">
                        <div class="stat text-center lg:text-left">
                            <div class="stat-title">"Happy Guests"</div>
                            <div class="stat-value text-primary">"15,000+"</div>
                            <div class="stat-desc">"Over 98% 5-star reviews"</div>
                        </div>

                        <div class="stat text-center lg:text-left">
                            <div class="stat-title">"Premium Stays"</div>
                            <div class="stat-value text-secondary">"1,200+"</div>
                            <div class="stat-desc">"Villas, apartments, and cabins"</div>
                        </div>

                        <div class="stat text-center lg:text-left">
                            <div class="stat-title">"Global Destinations"</div>
                            <div class="stat-value text-accent">"45+"</div>
                            <div class="stat-desc">"Countries and regions worldwide"</div>
                        </div>
                    </div>
                </div>
            </div>

            // Contact CTA Section
            <div class="py-16 text-center max-w-xl mx-auto px-4 flex flex-col gap-6 items-center">
                <h2 class="text-3xl font-bold">"Have Questions?"</h2>
                <p class="text-base-content/70">"Our support team is available 24/7 to assist you with booking issues, property queries, or host registration."</p>
                <a href="mailto:support@ourplaces.com" class="btn btn-primary btn-wide">"Get in Touch"</a>
            </div>
        </>
    }
}
