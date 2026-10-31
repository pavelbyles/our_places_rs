use std::str::FromStr;

use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{not_found, see_other},
        page, path_param,
    },
};
use uuid::Uuid;
use web_client::get_api_client;

use super::is_admin_account;
use crate::support::admin_only;

path_param!(id);

/// Archive (soft delete) or purge (hard delete).
#[derive(Debug, Deserialize)]
pub struct DeleteForm {
    /// `"true"` permanently deletes the account.
    #[serde(default)]
    pub purge: String,
}

fn back(notice: &str) -> topcoat::Error {
    see_other(format!("/admin/users?status=all&notice={notice}")).into()
}

/// Archives or purges an account. Administrator accounts and the signed-in
/// admin's own account are refused; purging an account with bookings,
/// listings or financial records fails and suggests archiving instead.
#[page(POST "/admin/users/{id}/delete")]
pub async fn delete_user(
    cx: &Cx,
    /// Whether to purge.
    Form(form): Form<DeleteForm>,
) -> Result<()> {
    let admin = admin_only(cx).await?;
    let id = Uuid::from_str(path_param::<Id>(cx)).map_err(|_| not_found())?;
    if id == admin.user_id {
        return Err(back("self"));
    }
    let api = get_api_client(cx);
    let target = api.get_user_profile(id).await.map_err(|_| not_found())?;
    if is_admin_account(&target) {
        return Err(back("admin-protected"));
    }
    let purge = form.purge == "true";
    let outcome = if purge {
        api.hard_delete_user(id).await.map(|_| "purged")
    } else {
        api.soft_delete_user(id).await.map(|_| "archived")
    };
    let notice = outcome.unwrap_or_else(|e| {
        tracing::warn!(user_id = %id, purge, "user delete failed: {e}");
        let text = e.to_string();
        if purge && (text.contains("linked") || text.contains("23503")) {
            "linked"
        } else {
            "failed"
        }
    });
    tracing::info!(user_id = %id, admin_id = %admin.user_id, purge, notice, "user delete");
    Err(back(notice))
}

/// Restores an archived account.
#[page(POST "/admin/users/{id}/restore")]
pub async fn restore_user(cx: &Cx) -> Result<()> {
    let admin = admin_only(cx).await?;
    let id = Uuid::from_str(path_param::<Id>(cx)).map_err(|_| not_found())?;
    let notice = match get_api_client(cx).restore_user(id).await {
        Ok(_) => "restored",
        Err(e) => {
            tracing::warn!(user_id = %id, "user restore failed: {e}");
            "failed"
        }
    };
    tracing::info!(user_id = %id, admin_id = %admin.user_id, notice, "user restore");
    Err(back(notice))
}
