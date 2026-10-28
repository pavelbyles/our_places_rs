//! Listing photo uploads: the browser asks for signed URLs here and then
//! uploads straight to Cloud Storage (no image bytes pass through the
//! portal). `image_worker` processes the uploads afterwards.

use common::models::{ImagePresignRequest, ImagePresignResponse, PendingImageMetadata};
use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{content::Json, error::not_found, path_param, route},
};
use uuid::Uuid;
use web_client::get_api_client;

use super::managed_listing;
use crate::support::staff;

path_param!(id);

/// Most photos accepted per upload.
pub const MAX_FILES: usize = 10;
/// Largest photo accepted, in bytes (15 MiB).
pub const MAX_BYTES: u64 = 15 * 1024 * 1024;
/// Accepted photo types.
pub const IMAGE_TYPES: [&str; 3] = ["image/jpeg", "image/png", "image/webp"];

/// One file the browser wants to upload.
#[derive(Deserialize)]
pub struct UploadFile {
    /// MIME type reported by the browser.
    pub content_type: String,
    /// Size in bytes.
    pub size_bytes: u64,
}

/// The browser's upload request.
#[derive(Deserialize)]
pub struct UploadRequest {
    /// Files to upload, in display order.
    pub files: Vec<UploadFile>,
}

/// Checks an upload request; `Err` carries the reason.
pub fn check_files(files: &[UploadFile]) -> std::result::Result<(), String> {
    if files.is_empty() || files.len() > MAX_FILES {
        return Err(format!("Choose between 1 and {MAX_FILES} photos."));
    }
    files.iter().try_for_each(|f| {
        if !IMAGE_TYPES.contains(&f.content_type.as_str()) {
            Err("Photos must be JPEG, PNG or WebP.".to_string())
        } else if f.size_bytes == 0 || f.size_bytes > MAX_BYTES {
            Err("Each photo must be under 15 MB.".to_string())
        } else {
            Ok(())
        }
    })
}

/// Signed upload URLs for a listing the user may manage (404 otherwise).
#[route(POST "/admin/listings/{id}/images/presign")]
pub async fn presign_images(
    cx: &Cx,
    Json(request): Json<UploadRequest>,
) -> Result<Json<Vec<ImagePresignResponse>>> {
    let user = staff(cx).await?;
    let raw = path_param::<Id>(cx);
    Uuid::parse_str(raw).map_err(|_| not_found())?;
    let listing = managed_listing(cx, &user, raw).await?;
    check_files(&request.files).map_err(web_client::app_error)?;
    let images = request
        .files
        .iter()
        .enumerate()
        .map(|(i, f)| PendingImageMetadata {
            client_file_id: i.to_string(),
            content_type: f.content_type.clone(),
            size_bytes: f.size_bytes,
            display_order: i32::try_from(i).unwrap_or(i32::MAX),
        })
        .collect();
    let signed = get_api_client(cx)
        .presign_listing_images(listing.id, &ImagePresignRequest { images })
        .await
        .map_err(web_client::app_error)?;
    tracing::info!(listing_id = %listing.id, count = signed.len(), "photo upload URLs issued");
    Ok(Json(signed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(content_type: &str, size_bytes: u64) -> UploadFile {
        UploadFile {
            content_type: content_type.into(),
            size_bytes,
        }
    }

    #[test]
    fn accepts_up_to_ten_supported_photos() {
        let files: Vec<_> = (0..10).map(|_| file("image/webp", 1024)).collect();
        assert_eq!(check_files(&files), Ok(()));
    }

    #[test]
    fn rejects_empty_too_many_wrong_type_and_oversized() {
        assert!(check_files(&[]).is_err());
        let eleven: Vec<_> = (0..11).map(|_| file("image/png", 1)).collect();
        assert!(check_files(&eleven).is_err());
        assert!(check_files(&[file("image/svg+xml", 10)]).is_err());
        assert!(check_files(&[file("image/jpeg", MAX_BYTES + 1)]).is_err());
        assert!(check_files(&[file("image/jpeg", 0)]).is_err());
    }
}
