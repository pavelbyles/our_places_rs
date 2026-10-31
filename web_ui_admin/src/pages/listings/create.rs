use topcoat::{
    Result,
    context::Cx,
    router::{content::Form, error::see_other, page, path_param},
    view::{View, view},
};
use web_client::{get_api_client, session::AdminRole};

use super::form::{ListingFields, api_rejection_messages, listing_form};
use super::{managed_listing, owner_options};
use crate::support::staff;

path_param!(slug);

/// A blank listing studio form.
#[page("/admin/listings/new")]
pub async fn new_listing_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let owners = owner_options(cx, &user).await;
    let admin = user.role == AdminRole::Admin;
    let fields = ListingFields::blank(user.user_id);
    Ok(view! {
        <div class="space-y-6">
            <h1 class="text-2xl font-semibold">"New listing"</h1>
            listing_form(action: "/admin/listings".into(), submit_label: "Create listing".into(), fields: fields, owners: owners, can_set_commission: admin, errors: Vec::new())
        </div>
    })
}

/// The studio form pre-filled from an existing listing.
#[page("/admin/listings/clone/{slug}")]
pub async fn clone_listing_page(cx: &Cx) -> Result<impl View> {
    let user = staff(cx).await?;
    let source = managed_listing(cx, &user, path_param::<Slug>(cx)).await?;
    let owners = owner_options(cx, &user).await;
    let admin = user.role == AdminRole::Admin;
    let mut fields = ListingFields::from_listing(&source);
    fields.name = format!("Copy of {}", source.name);
    if !admin {
        fields.owner_id = user.user_id.to_string();
    }
    Ok(view! {
        <div class="space-y-6">
            <h1 class="text-2xl font-semibold">(format!("Clone {}", source.name))</h1>
            listing_form(action: "/admin/listings".into(), submit_label: "Create listing".into(), fields: fields, owners: owners, can_set_commission: admin, errors: Vec::new())
        </div>
    })
}

/// Creates a listing. Hosts always own what they create; only admins may
/// choose the owner or set the commission.
#[page(POST "/admin/listings")]
pub async fn create_listing(
    cx: &Cx,
    /// The studio form.
    Form(fields): Form<ListingFields>,
) -> Result<impl View> {
    let user = staff(cx).await?;
    let admin = user.role == AdminRole::Admin;
    let forced_owner = (!admin).then_some(user.user_id);
    let errors = match fields.validate(forced_owner, admin) {
        Ok(valid) => match get_api_client(cx).create_listing(&valid.into_new()).await {
            Ok(created) => {
                tracing::info!(listing_id = %created.id, "listing created");
                return Err(see_other(format!("/admin/listings/{}", created.id)).into());
            }
            Err(e) => {
                tracing::warn!(error = %e, "listing create rejected");
                if e.to_string().contains("host profile") {
                    vec!["The owner needs a host profile before they can list a property.".into()]
                } else {
                    api_rejection_messages(&e).unwrap_or_else(|| {
                        vec![
                            "The listing could not be created. Check the values and try again."
                                .into(),
                        ]
                    })
                }
            }
        },
        Err(errors) => errors,
    };
    let owners = owner_options(cx, &user).await;
    Ok(view! {
        <div class="space-y-6">
            <h1 class="text-2xl font-semibold">"New listing"</h1>
            listing_form(action: "/admin/listings".into(), submit_label: "Create listing".into(), fields: fields, owners: owners, can_set_commission: admin, errors: errors)
        </div>
    })
}
