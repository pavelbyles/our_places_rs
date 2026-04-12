use crate::auth::VerifyEmailCode;
use leptos::prelude::*;
use leptos_router::hooks::use_query_map;

#[component]
pub fn VerifyPage() -> impl IntoView {
    let query = use_query_map();
    let email = move || query.get().get("email").unwrap_or_default();
    
    let auth = use_context::<crate::app::AuthContext>().expect("AuthContext should be provided");
    let verify_action = ServerAction::<VerifyEmailCode>::new();

    Effect::new(move || {
        if let Some(Ok(_)) = verify_action.value().get() {
            auth.refresh();
        }
    });
    
    view! {
        <div class="min-h-screen flex items-center justify-center bg-base-200">
            <div class="card w-full max-w-md bg-base-100 shadow-xl overflow-hidden">
                <div class="card-body p-0">
                    <div class="bg-primary text-primary-content p-8 text-center">
                        <h2 class="text-3xl font-bold">"Verify Email"</h2>
                        <p class="mt-2 opacity-90">"Enter the 6-digit code sent to your email"</p>
                    </div>
                    
                    <div class="p-8">
                        <p class="text-center mb-6 text-sm opacity-70">
                            "We've sent a verification code to " <strong>{email}</strong>
                        </p>

                        <ActionForm action=verify_action>
                            <input type="hidden" name="email" value=email />
                            
                            <div class="form-control w-full">
                                <label class="label">
                                    <span class="label-text font-semibold text-center w-full">"Verification Code"</span>
                                </label>
                                <input 
                                    type="text" 
                                    name="code" 
                                    placeholder="XXXXXX" 
                                    class="input input-bordered w-full text-center text-2xl tracking-[0.5em] font-mono" 
                                    maxlength="6"
                                    required 
                                    autofocus
                                />
                            </div>

                            <button type="submit" class="btn btn-primary w-full mt-8" disabled=move || verify_action.pending().get()>
                                {move || if verify_action.pending().get() { "Verifying..." } else { "Verify & Login" }}
                            </button>

                            {move || verify_action.value().get().map(|res| match res {
                                Err(e) => view! {
                                    <div class="alert alert-error mt-4 shadow-md text-sm">
                                        <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
                                        <span>{e.to_string()}</span>
                                    </div>
                                }.into_any(),
                                Ok(_) => view! {
                                    <div class="alert alert-success mt-4 shadow-md text-sm">
                                        <svg xmlns="http://www.w3.org/2000/svg" class="stroke-current shrink-0 h-6 w-6" fill="none" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" /></svg>
                                        <span>"Email verified successfully! Logging you in..."</span>
                                    </div>
                                }.into_any(),
                            })}
                        </ActionForm>

                        <div class="text-center mt-8">
                            <p class="text-xs opacity-60 mb-2">"Didn't receive the code?"</p>
                            <button class="btn btn-link btn-xs no-underline">"Resend Code (Not implemented)"</button>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
