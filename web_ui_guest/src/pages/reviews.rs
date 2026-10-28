use common::models::{NewReviewRequest, ReviewTokenInfoResponse};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{not_found, see_other},
        page, parse_query_params,
    },
    view::{View, component, view},
};
use web_ui_common::components::star_rating::star_rating_input;

/// The categories guests rate, as (form field prefix, label).
const CATEGORIES: [(&str, &str); 4] = [
    ("cleanliness", "Cleanliness"),
    ("accuracy", "Accuracy"),
    ("location", "Location"),
    ("value", "Value"),
];

#[derive(Deserialize, Default)]
struct TokenQuery {
    token: Option<String>,
}

/// Review form fields.
#[derive(Deserialize)]
pub struct ReviewForm {
    /// The review token from the post-stay email.
    pub token: String,
    /// Cleanliness, 1–5.
    pub cleanliness_rating: i32,
    /// Accuracy, 1–5.
    pub accuracy_rating: i32,
    /// Location, 1–5.
    pub location_rating: i32,
    /// Value, 1–5.
    pub value_rating: i32,
    /// Public review.
    #[serde(default)]
    pub public_review_text: String,
    /// Private note to the host.
    #[serde(default)]
    pub private_host_feedback: String,
}

/// Review tokens are URL-safe; anything else is rejected before it is put
/// into a backend URL.
fn valid_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 128
        && token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Looks the token up with `listing_api`. Unknown, expired and already-used
/// tokens all become a 404.
async fn token_info(token: &str) -> Result<ReviewTokenInfoResponse> {
    if !valid_token(token) {
        return Err(not_found().into());
    }
    common::app_client::get_review_token_info(token)
        .await
        .ok()
        .filter(|info| info.is_valid)
        .ok_or_else(|| not_found().into())
}

#[component]
async fn review_form(
    info: ReviewTokenInfoResponse,
    token: String,
    error: Option<String>,
) -> Result<impl View> {
    let label = "block text-xs font-semibold tracking-wide text-muted-foreground uppercase";
    let input = "mt-1 block w-full rounded-lg border border-border bg-card px-3 py-2 text-sm text-foreground normal-case tracking-normal font-normal";
    Ok(view! {
        <div class="mx-auto max-w-2xl space-y-6">
            <div class="space-y-1">
                <h1 class="font-serif text-[clamp(28px,4vw,40px)]">"Rate your stay"</h1>
                <p class="text-sm text-muted-foreground">(format!("{}, {} → {}", info.listing_name, info.check_in.format("%-d %b %Y"), info.check_out.format("%-d %b %Y")))</p>
            </div>
            if let Some(error) = error {
                <p role="alert" class="rounded-lg border border-destructive p-3 text-sm text-destructive">(error)</p>
            }
            <form method="post" action="/reviews/submit" class="space-y-6 rounded-[var(--radius-surface,16px)] border border-border bg-card p-6">
                <input type="hidden" name="token" value=(token) />
                <div class="grid gap-6 sm:grid-cols-2">
                    for (field, title) in CATEGORIES {
                        star_rating_input(name: format!("{field}_rating"), label: title.to_string())
                    }
                </div>
                <label class=(label) for="public-review">"Your review (shown publicly)"
                    <textarea id="public-review" name="public_review_text" rows="4" maxlength="2000" class=(input)></textarea>
                </label>
                <label class=(label) for="private-feedback">"Private note to your host (optional)"
                    <textarea id="private-feedback" name="private_host_feedback" rows="2" maxlength="1000" class=(input)></textarea>
                </label>
                <button type="submit" class="h-11 rounded-full bg-primary px-6 text-sm font-medium text-primary-foreground">"Submit review"</button>
            </form>
        </div>
    })
}

/// The review form for a verified stay (`?token=` from the post-stay email).
#[page("/reviews/submit")]
pub async fn review_page(cx: &Cx) -> Result<impl View> {
    let token = parse_query_params::<TokenQuery>(cx)
        .ok()
        .and_then(|q| q.token)
        .unwrap_or_default();
    let info = token_info(&token).await?;
    Ok(view! { review_form(info: info, token: token, error: None) })
}

/// Submits the review to `listing_api`.
#[page(POST "/reviews/submit")]
pub async fn review_submit(
    cx: &Cx,
    /// The review form.
    Form(form): Form<ReviewForm>,
) -> Result<impl View> {
    let _ = cx;
    let info = token_info(&form.token).await?;
    let ratings = [
        form.cleanliness_rating,
        form.accuracy_rating,
        form.location_rating,
        form.value_rating,
    ];
    let error = if ratings.iter().any(|r| !(1..=5).contains(r)) {
        "Please rate every category from 1 to 5.".to_string()
    } else {
        let request = NewReviewRequest {
            token: form.token.clone(),
            cleanliness_rating: form.cleanliness_rating,
            accuracy_rating: form.accuracy_rating,
            location_rating: form.location_rating,
            value_rating: form.value_rating,
            public_review_text: Some(form.public_review_text.trim().to_string())
                .filter(|t| !t.is_empty()),
            private_host_feedback: Some(form.private_host_feedback.trim().to_string())
                .filter(|t| !t.is_empty()),
        };
        match common::app_client::submit_review(&form.token, &request).await {
            Ok(()) => return Err(see_other("/reviews/success").into()),
            Err(e) => {
                tracing::info!("review rejected: {e}");
                "Your review could not be saved. Please try again.".to_string()
            }
        }
    };
    let token = form.token;
    Ok(view! { review_form(info: info, token: token, error: Some(error)) })
}

/// Thank-you page after a review.
#[page("/reviews/success")]
pub async fn review_success(cx: &Cx) -> Result<impl View> {
    let _ = cx;
    Ok(view! {
        <div class="mx-auto max-w-xl space-y-4 py-10 text-center">
            <h1 class="font-serif text-[clamp(28px,4vw,40px)]">"Thank you for your review"</h1>
            <p class="text-muted-foreground">"Your verified review helps future guests choose their stay."</p>
            <div class="flex justify-center gap-3">
                <a href="/listings" class="inline-flex h-11 items-center rounded-full bg-primary px-6 text-sm font-medium text-primary-foreground no-underline">"Explore more listings"</a>
                <a href="/bookings" class="inline-flex h-11 items-center rounded-full border border-border px-6 text-sm font-medium no-underline">"My bookings"</a>
            </div>
        </div>
    })
}
