use common::tax_document::TaxDocumentResponse;
use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use topcoat::{
    Result,
    context::Cx,
    router::{error::RouterErrorExt, page, path_param},
    view::{View, view},
};
use uuid::Uuid;
use web_app_common_tc::components::tax_document_view::tax_document_view;
use web_app_common_tc::get_api_client;

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    exp: usize,
}

fn generate_jwt_for_user(user_id: Uuid) -> String {
    let secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| "secret".to_string());
    let claims = Claims {
        sub: user_id,
        exp: (chrono::Utc::now() + chrono::Duration::hours(1)).timestamp() as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .unwrap_or_default()
}

path_param!(id);
path_param!(number);

#[page("/bookings/{id}/documents/{number}")]
pub async fn booking_document_page(cx: &Cx) -> Result<impl View> {
    render_booking_document_page(cx).await
}

pub async fn render_booking_document_page(cx: &Cx) -> Result<impl View> {
    let __cx = cx;
    // 1. Authorization: Only authenticated guest or admin can view
    let guest_session = web_app_common_tc::auth::get_guest_session(cx).await;
    let admin_session = if guest_session.is_none() {
        web_app_common_tc::auth::get_admin_session(cx).await
    } else {
        None
    };

    let user_id = guest_session
        .and_then(|s| s.id)
        .or_else(|| admin_session.and_then(|s| s.id))
        .ok_or_not_found()?;

    // 2. Parse path params
    let id_str: &str = path_param::<Id>(cx);
    let number: &str = path_param::<Number>(cx);
    let booking_id = Uuid::parse_str(id_str).map_err(|_| topcoat::router::error::not_found())?;

    // 3. Fetch document with caller JWT token
    let api = get_api_client(cx);
    let token = generate_jwt_for_user(user_id);
    let doc: TaxDocumentResponse = api
        .get_booking_document_by_number(booking_id, number, Some(&token))
        .await
        .map_err(|_| topcoat::router::error::not_found())?;

    Ok(view! {
        <div class="max-w-4xl mx-auto px-4 py-8 space-y-6">
            <div class="no-print flex items-center justify-between">
                <a href="/bookings" class="btn btn-ghost btn-sm gap-2 font-medium">
                    "← Back to Bookings"
                </a>
                <button
                    type="button"
                    onclick="window.print()"
                    class="btn btn-primary btn-sm gap-2 font-bold shadow-sm"
                >
                    <svg xmlns="http://www.w3.org/2000/svg" class="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 17h2a2 2 0 002-2v-4a2 2 0 00-2-2H5a2 2 0 00-2 2v4a2 2 0 002 2h2m2 4h6a2 2 0 002-2v-4a2 2 0 00-2-2H9a2 2 0 00-2 2v4a2 2 0 002 2zm8-12V5a2 2 0 00-2-2H9a2 2 0 00-2 2v4h10z" />
                    </svg>
                    "Print / Download PDF"
                </button>
            </div>
            tax_document_view(doc: doc)
        </div>
    })
}
