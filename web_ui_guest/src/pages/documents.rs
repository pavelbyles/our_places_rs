use topcoat::{
    Result,
    context::Cx,
    router::{error::not_found, page, path_param},
    view::{View, view},
};
use web_client::{get_api_client, session::require_guest};
use web_ui_common::components::domain::tax_document_view::tax_document_view;

use crate::support::{owned_booking, parse_booking_id, service_token};

path_param!(id);
path_param!(number);

/// A tax document for one of the guest's bookings. Signed-out visitors and
/// other guests get a 404 (as in the legacy portal), never a hint that the
/// document exists.
#[page("/bookings/{id}/documents/{number}")]
pub async fn document_page(cx: &Cx) -> Result<impl View> {
    let id = parse_booking_id(path_param::<Id>(cx))?;
    let number = path_param::<Number>(cx).to_string();
    let guest = require_guest(cx).await.map_err(|_| not_found())?;
    owned_booking(cx, &guest, id).await?;
    let token = service_token(cx, &guest)?;
    let doc = get_api_client(cx)
        .get_booking_document_by_number(id, &number, Some(&token))
        .await
        .map_err(|_| not_found())?;
    Ok(view! {
        <div class="mx-auto max-w-4xl space-y-6">
            <div class="no-print flex items-center justify-between">
                <a href="/bookings" class="text-sm">"← My bookings"</a>
                <button type="button" data-print="" class="h-10 rounded-full bg-primary px-4 text-sm font-medium text-primary-foreground">"Print / Download PDF"</button>
            </div>
            tax_document_view(doc: doc)
        </div>
    })
}
