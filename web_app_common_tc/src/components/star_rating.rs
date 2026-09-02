use topcoat::{
    Result,
    view::{component, view},
};

#[component]
pub async fn star_rating(rating: Option<f64>, review_count: Option<i64>) -> Result {
    let formatted_rating = rating.map(|r| format!("{:.1}", r));
    let count_text = review_count
        .map(|c| format!("({} reviews)", c))
        .unwrap_or_default();

    view! {
        if let Some(r_str) = formatted_rating {
            <div class="flex items-center gap-1.5 font-medium text-sm">
                <div class="badge badge-primary gap-1 font-bold">
                    <svg xmlns="http://www.w3.org/2000/svg" class="h-3.5 w-3.5 fill-current" viewBox="0 0 20 20">
                        <path d="M9.049 2.927c.3-.921 1.603-.921 1.902 0l1.07 3.292a1 1 0 00.95.69h3.462c.969 0 1.371 1.24.588 1.81l-2.8 2.034a1 1 0 00-.364 1.118l1.07 3.292c.3.921-.755 1.688-1.54 1.118l-2.8-2.034a1 1 0 00-1.175 0l-2.8 2.034c-.784.57-1.838-.197-1.539-1.118l1.07-3.292a1 1 0 00-.364-1.118L2.98 8.72c-.783-.57-.38-1.81.588-1.81h3.461a1 1 0 00.951-.69l1.07-3.292z" />
                    </svg>
                    (r_str)
                </div>
                if !count_text.is_empty() {
                    <span class="text-xs text-base-content/70">(count_text)</span>
                }
            </div>
        } else {
            <div class="badge badge-outline gap-1 text-xs font-semibold text-base-content/60">
                "New"
            </div>
        }
    }
}
