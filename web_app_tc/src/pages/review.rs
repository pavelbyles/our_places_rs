use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::view,
};

#[page("/reviews/submit")]
pub async fn review_submit_page(_cx: &Cx) -> Result {
    view! {
        <div class="max-w-2xl mx-auto px-4 py-10 space-y-8">
            <div class="card bg-base-100 shadow-xl border border-base-200 rounded-3xl overflow-hidden">
                <div class="bg-primary text-primary-content p-8 text-center space-y-2">
                    <h1 class="text-3xl font-black">"Rate Your Stay"</h1>
                    <p class="text-sm opacity-90">"Share your verified experience with the host and future travelers."</p>
                </div>

                <div class="p-8 space-y-8">
                    <form action="/reviews/success" method="GET" class="space-y-6">
                        <div class="grid grid-cols-1 sm:grid-cols-2 gap-6 bg-base-200/60 p-6 rounded-2xl border border-base-300">
                            // Cleanliness
                            <div class="space-y-2 text-center">
                                <label class="text-xs font-bold uppercase block text-base-content/70">"Cleanliness"</label>
                                <div class="rating rating-md">
                                    <input type="radio" name="cleanliness" value="1" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="cleanliness" value="2" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="cleanliness" value="3" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="cleanliness" value="4" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="cleanliness" value="5" class="mask mask-star bg-amber-400" checked=(true) />
                                </div>
                            </div>

                            // Accuracy
                            <div class="space-y-2 text-center">
                                <label class="text-xs font-bold uppercase block text-base-content/70">"Accuracy"</label>
                                <div class="rating rating-md">
                                    <input type="radio" name="accuracy" value="1" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="accuracy" value="2" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="accuracy" value="3" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="accuracy" value="4" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="accuracy" value="5" class="mask mask-star bg-amber-400" checked=(true) />
                                </div>
                            </div>

                            // Location
                            <div class="space-y-2 text-center">
                                <label class="text-xs font-bold uppercase block text-base-content/70">"Location"</label>
                                <div class="rating rating-md">
                                    <input type="radio" name="location" value="1" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="location" value="2" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="location" value="3" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="location" value="4" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="location" value="5" class="mask mask-star bg-amber-400" checked=(true) />
                                </div>
                            </div>

                            // Value
                            <div class="space-y-2 text-center">
                                <label class="text-xs font-bold uppercase block text-base-content/70">"Value"</label>
                                <div class="rating rating-md">
                                    <input type="radio" name="value" value="1" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="value" value="2" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="value" value="3" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="value" value="4" class="mask mask-star bg-amber-400" />
                                    <input type="radio" name="value" value="5" class="mask mask-star bg-amber-400" checked=(true) />
                                </div>
                            </div>
                        </div>

                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Public Review"</label>
                            <textarea
                                name="public_review"
                                placeholder="Share your experience with future travelers. What was the highlight of your stay?"
                                class="textarea textarea-bordered w-full h-32 text-sm"
                            ></textarea>
                        </div>

                        <div>
                            <label class="label text-xs font-bold uppercase text-base-content/70">"Private Host Feedback (Optional)"</label>
                            <textarea
                                name="private_feedback"
                                placeholder="Any suggestions or private notes for your host?"
                                class="textarea textarea-bordered w-full h-24 text-sm"
                            ></textarea>
                        </div>

                        <button type="submit" class="btn btn-primary btn-block text-lg font-bold">
                            "Submit Verified Review →"
                        </button>
                    </form>
                </div>
            </div>
        </div>
    }
}

#[page("/reviews/success")]
pub async fn review_success_page(_cx: &Cx) -> Result {
    view! {
        <div class="max-w-2xl mx-auto px-4 py-16 space-y-8 text-center">
            <div class="card bg-base-100 shadow-xl border border-base-200 p-8 md:p-12 rounded-3xl space-y-6">
                <div class="w-16 h-16 bg-success/20 text-success rounded-full flex items-center justify-center mx-auto text-3xl font-bold">
                    "✓"
                </div>
                <div class="space-y-2">
                    <h1 class="text-3xl font-serif font-bold text-base-content">"Thank You for Your Review!"</h1>
                    <p class="text-sm text-base-content/70 max-w-md mx-auto">
                        "Your verified review has been submitted and aggregated into the property's overall rating."
                    </p>
                </div>
                <div class="flex flex-col sm:flex-row justify-center gap-3 pt-4">
                    <a href="/listings" class="btn btn-primary rounded-xl">"Explore More Villas"</a>
                    <a href="/bookings" class="btn btn-outline rounded-xl">"View My Bookings"</a>
                </div>
            </div>
        </div>
    }
}
