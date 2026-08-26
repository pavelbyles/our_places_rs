use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use web_app_common::reviews::get_review_token_info_server;

#[server]
pub async fn submit_review_action(
    token: String,
    cleanliness_rating: i32,
    accuracy_rating: i32,
    location_rating: i32,
    value_rating: i32,
    public_review_text: Option<String>,
    private_host_feedback: Option<String>,
) -> Result<(), ServerFnError> {
    let req = common::models::NewReviewRequest {
        token: token.clone(),
        cleanliness_rating,
        accuracy_rating,
        location_rating,
        value_rating,
        public_review_text,
        private_host_feedback,
    };
    web_app_common::reviews::submit_review_server(token, req).await
}

#[component]
pub fn ReviewSubmitPage() -> impl IntoView {
    let params = use_params_map();
    let token = move || params.get().get("token").unwrap_or_default();

    let token_info_resource = Resource::new(token, |t| async move {
        get_review_token_info_server(t.clone()).await
    });

    let submit_action = ServerAction::<SubmitReviewAction>::new();

    view! {
        <div class="min-h-screen bg-base-200 py-12 px-4 sm:px-6 lg:px-8">
            <div class="max-w-2xl mx-auto">
                <Suspense fallback=move || view! { <div class="flex justify-center"><span class="loading loading-spinner loading-lg text-primary"></span></div> }>
                    {move || match token_info_resource.get() {
                        Some(Ok(info)) => {
                            if !info.is_valid {
                                view! {
                                    <div class="card bg-base-100 shadow-xl">
                                        <div class="card-body text-center items-center">
                                            <svg xmlns="http://www.w3.org/2000/svg" class="h-16 w-16 text-error mb-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                                            </svg>
                                            <h2 class="card-title text-2xl">"Link Invalid or Expired"</h2>
                                            <p class="text-base-content/70 mt-2">
                                                {move || match info.error_code.as_deref() {
                                                    Some("ALREADY_USED") => "This review link has already been used. You can only submit one review per stay.",
                                                    Some("EXPIRED") => "This review link has expired. You only have 15 days after your stay to submit a review.",
                                                    Some("NOT_YET_VALID") => "This review link is not yet valid. Please try again later.",
                                                    _ => "This review link is invalid or cannot be processed at this time."
                                                }}
                                            </p>
                                        </div>
                                    </div>
                                }.into_any()
                            } else if submit_action.value().get().map(|v| v.is_ok()).unwrap_or(false) {
                                view! {
                                    <div class="card bg-base-100 shadow-xl">
                                        <div class="card-body text-center items-center py-16">
                                            <svg xmlns="http://www.w3.org/2000/svg" class="h-20 w-20 text-success mb-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                                            </svg>
                                            <h2 class="card-title text-3xl font-bold text-success">"Review Submitted!"</h2>
                                            <p class="text-base-content/70 mt-4 text-lg">
                                                "Thank you for sharing your experience at " <strong>{info.listing_name.clone()}</strong> "."
                                            </p>
                                            <p class="text-base-content/70 mt-2">
                                                "Your feedback helps other guests and our hosts."
                                            </p>
                                            <div class="mt-8">
                                                <a href="/" class="btn btn-primary">"Back to Home"</a>
                                            </div>
                                        </div>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div class="card bg-base-100 shadow-xl">
                                        <div class="card-body p-6 md:p-10">
                                            <div class="text-center mb-8">
                                                <h1 class="text-3xl font-bold text-base-content mb-2">"Rate your stay"</h1>
                                                <p class="text-base-content/70">
                                                    "Hi " <strong>{info.guest_first_name.clone()}</strong> ", how was your stay at "
                                                    <strong>{info.listing_name.clone()}</strong> "?"
                                                </p>
                                                <p class="text-xs text-base-content/50 mt-1">
                                                    {info.check_in.format("%b %d, %Y").to_string()} " - " {info.check_out.format("%b %d, %Y").to_string()}
                                                </p>
                                            </div>

                                            <div class="space-y-8">
                                                <ActionForm action=submit_action>
                                                    <input type="hidden" name="token" value=token />

                                                <div class="grid grid-cols-1 md:grid-cols-2 gap-8 bg-base-200/50 p-6 rounded-box">
                                                    <RatingInput name="cleanliness_rating" label="Cleanliness" />
                                                    <RatingInput name="accuracy_rating" label="Accuracy" />
                                                    <RatingInput name="location_rating" label="Location" />
                                                    <RatingInput name="value_rating" label="Value" />
                                                </div>

                                                <div class="space-y-4">
                                                    <fieldset class="fieldset w-full">
                                                        <legend class="fieldset-legend font-semibold text-base text-base-content">"Public Review"</legend>
                                                        <p class="text-xs text-base-content/60 mb-2">"Share your experience with future guests. (Optional)"</p>
                                                        <textarea
                                                            name="public_review_text"
                                                            class="textarea textarea-bordered h-32 w-full resize-none text-base"
                                                            placeholder="What was the highlight of your stay?"
                                                        ></textarea>
                                                    </fieldset>
                                                </div>

                                                <div class="space-y-4">
                                                    <fieldset class="fieldset w-full">
                                                        <legend class="fieldset-legend font-semibold text-base text-base-content">"Private Feedback for Host"</legend>
                                                        <p class="text-xs text-base-content/60 mb-2">"This won't be shown publicly. (Optional)"</p>
                                                        <textarea
                                                            name="private_host_feedback"
                                                            class="textarea textarea-bordered h-24 w-full resize-none text-base"
                                                            placeholder="Any suggestions for the host to improve?"
                                                        ></textarea>
                                                    </fieldset>
                                                </div>

                                                {move || submit_action.value().get().map(|res| match res {
                                                    Err(e) => view! {
                                                        <div class="alert alert-error">
                                                            <span>{e.to_string()}</span>
                                                        </div>
                                                    }.into_any(),
                                                    Ok(_) => view! { <div></div> }.into_any(),
                                                })}

                                                <button
                                                    type="submit"
                                                    class="btn btn-primary w-full text-lg h-14"
                                                    disabled=move || submit_action.pending().get()
                                                >
                                                    {move || if submit_action.pending().get() { "Submitting..." } else { "Submit Review" }}
                                                </button>
                                                </ActionForm>
                                            </div>
                                        </div>
                                    </div>
                                }.into_any()
                            }
                        },
                        Some(Err(e)) => view! {
                            <div class="alert alert-error shadow-lg">
                                <div>
                                    <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current flex-shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
                                    <span>"Error loading review page: " {e.to_string()}</span>
                                </div>
                            </div>
                        }.into_any(),
                        None => view! { <div></div> }.into_any(),
                    }}
                </Suspense>
            </div>
        </div>
    }
}

#[component]
fn RatingInput(name: &'static str, label: &'static str) -> impl IntoView {
    view! {
        <div class="flex flex-col items-center p-4 bg-base-100 rounded-xl shadow-sm border border-base-300">
            <span class="font-medium text-base-content/80 mb-3">{label}</span>
            <div class="rating rating-lg">
                <input type="radio" name=name value="1" class="mask mask-star bg-orange-400" />
                <input type="radio" name=name value="2" class="mask mask-star bg-orange-400" />
                <input type="radio" name=name value="3" class="mask mask-star bg-orange-400" checked />
                <input type="radio" name=name value="4" class="mask mask-star bg-orange-400" />
                <input type="radio" name=name value="5" class="mask mask-star bg-orange-400" />
            </div>
        </div>
    }
}
