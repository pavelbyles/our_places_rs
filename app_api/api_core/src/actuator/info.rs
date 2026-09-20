use actix_web::HttpResponse;
use chrono::Utc;

use crate::actuator::models::{AppInfoResponse, AppMetadata, BuildMetadata, GitMetadata};

/// Returns compile-time build, git, and application metadata.
#[utoipa::path(
    get,
    path = "/info",
    tag = "actuator",
    responses(
        (status = 200, description = "Application and build metadata", body = AppInfoResponse)
    )
)]
pub async fn info_handler() -> HttpResponse {
    let app = AppMetadata {
        name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        description: env!("CARGO_PKG_DESCRIPTION").to_string(),
    };

    let git = GitMetadata {
        branch: option_env!("APP_GIT_BRANCH")
            .unwrap_or("unknown")
            .to_string(),
        commit_hash: option_env!("APP_GIT_SHA").unwrap_or("unknown").to_string(),
        commit_short_hash: option_env!("APP_GIT_SHORT_SHA")
            .unwrap_or("unknown")
            .to_string(),
        commit_timestamp: option_env!("APP_GIT_COMMIT_TIMESTAMP")
            .unwrap_or("")
            .to_string(),
    };

    let build = BuildMetadata {
        timestamp: Utc::now().to_rfc3339(),
        rustc_version: option_env!("APP_RUSTC_VERSION")
            .unwrap_or("unknown")
            .to_string(),
        target_triple: option_env!("APP_TARGET_TRIPLE")
            .unwrap_or("unknown")
            .to_string(),
        profile: option_env!("APP_PROFILE").unwrap_or("unknown").to_string(),
    };

    HttpResponse::Ok().json(AppInfoResponse { app, git, build })
}
