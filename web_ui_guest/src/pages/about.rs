use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::{View, view},
};

/// About Our Places.
#[page("/about")]
pub async fn about_page(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <article class="mx-auto max-w-3xl space-y-8">
            <h1 class="font-serif text-[clamp(36px,5vw,56px)] leading-tight">"Authentic Caribbean Luxury"</h1>
            <p class="text-lg text-muted-foreground">"Our Places was born from a desire to showcase the very best of Jamaica's luxury villas and private apartments. We partner directly with property owners and staff to curate authentic, unforgettable getaways."</p>
            <div class="grid gap-8 sm:grid-cols-3">
                <section class="space-y-2">
                    <h2 class="font-medium">"Direct owner stays"</h2>
                    <p class="text-sm text-muted-foreground">"No middlemen or inflated markups. Talk directly with passionate local hosts."</p>
                </section>
                <section class="space-y-2">
                    <h2 class="font-medium">"Verified reviews"</h2>
                    <p class="text-sm text-muted-foreground">"Only guests who stayed can review, so every rating reflects a real visit."</p>
                </section>
                <section class="space-y-2">
                    <h2 class="font-medium">"Prices you understand"</h2>
                    <p class="text-sm text-muted-foreground">"See every stay in USD, JMD, EUR, GBP or CAD, with Jamaican GCT itemised."</p>
                </section>
            </div>
        </article>
    })
}
