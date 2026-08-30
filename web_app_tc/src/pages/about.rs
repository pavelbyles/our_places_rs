use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};

#[page("/about")]
pub async fn about_page(_cx: &Cx) -> Result {
    view! {
        <div class="max-w-5xl mx-auto px-2 py-10 space-y-16">
            // Hero
            <div class="text-center max-w-3xl mx-auto space-y-4">
                <span class="badge badge-primary uppercase tracking-wider font-bold">"Our Story"</span>
                <h1 class="text-4xl md:text-5xl font-black tracking-tight">"Authentic Caribbean Luxury"</h1>
                <p class="text-base-content/80 text-lg leading-relaxed">
                    "Our Places was born from a desire to showcase the very best of Jamaica's luxury villas and private apartments. We partner directly with property owners and staff to curate authentic, unforgettable getaways."
                </p>
            </div>

            // Values
            <div class="grid grid-cols-1 md:grid-cols-3 gap-8">
                <div class="card bg-base-200 p-8 rounded-3xl space-y-3">
                    <div class="text-3xl">"🌴"</div>
                    <h3 class="text-xl font-bold">"Direct Owner Stays"</h3>
                    <p class="text-sm text-base-content/70">"No middlemen or inflated booking markups. Direct communication with passionate local hosts."</p>
                </div>
                <div class="card bg-base-200 p-8 rounded-3xl space-y-3">
                    <div class="text-3xl">"✨"</div>
                    <h3 class="text-xl font-bold">"Verified Quality"</h3>
                    <p class="text-sm text-base-content/70">"Each property is personally inspected and maintained to the highest international luxury standards."</p>
                </div>
                <div class="card bg-base-200 p-8 rounded-3xl space-y-3">
                    <div class="text-3xl">"🛡️"</div>
                    <h3 class="text-xl font-bold">"Zero-Double Booking"</h3>
                    <p class="text-sm text-base-content/70">"Guaranteed atomic availability with instant 15-minute reservation locking on Cloud Run."</p>
                </div>
            </div>

            // Stats
            <div class="stats stats-vertical lg:stats-horizontal shadow-md w-full bg-base-100 border border-base-200 rounded-3xl">
                <div class="stat text-center">
                    <div class="stat-title font-bold">"Villas Managed"</div>
                    <div class="stat-value text-primary">"5"</div>
                    <div class="stat-desc">"Luxury Jamaican properties"</div>
                </div>
                <div class="stat text-center">
                    <div class="stat-title font-bold">"Verified Stays"</div>
                    <div class="stat-value text-secondary">"1,500+"</div>
                    <div class="stat-desc">"Guests from around the world"</div>
                </div>
                <div class="stat text-center">
                    <div class="stat-title font-bold">"Average Rating"</div>
                    <div class="stat-value text-accent">"4.95 ★"</div>
                    <div class="stat-desc">"Across all 5 destinations"</div>
                </div>
            </div>

            // CTA
            <div class="card bg-primary text-primary-content p-10 rounded-3xl text-center space-y-4 shadow-xl">
                <h2 class="text-3xl font-extrabold">"Ready for Paradise?"</h2>
                <p class="text-sm opacity-90 max-w-xl mx-auto">
                    "Book your private Caribbean sanctuary today and let our dedicated concierges take care of the rest."
                </p>
                <div>
                    <a href="/listings" class="btn btn-secondary btn-wide font-bold">"Browse Luxury Villas"</a>
                </div>
            </div>
        </div>
    }
}
