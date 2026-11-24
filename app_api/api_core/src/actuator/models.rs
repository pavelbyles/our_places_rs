use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

/// Whether something is healthy.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum HealthStatus {
    /// Healthy.
    Up,
    /// Unhealthy.
    Down,
}

/// Details of a health check.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct ComponentDetails {
    /// Which database was checked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// How long the check took, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    /// Why the check failed, if it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Health of one component.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct ComponentHealth {
    /// Whether the component is up.
    pub status: HealthStatus,
    /// Details of the check, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ComponentDetails>,
}

/// Overall health.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct HealthResponse {
    /// Overall status.
    pub status: HealthStatus,
    /// Status of each component.
    pub components: HashMap<String, ComponentHealth>,
}

/// Result of a liveness or readiness probe.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct ProbeResponse {
    /// Probe result.
    pub status: HealthStatus,
}

/// About the application.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AppMetadata {
    /// Crate name.
    pub name: String,
    /// Crate version.
    pub version: String,
    /// Crate description.
    pub description: String,
}

/// About the commit it was built from.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct GitMetadata {
    /// Branch it was built from.
    pub branch: String,
    /// Full commit hash.
    pub commit_hash: String,
    /// Short commit hash.
    pub commit_short_hash: String,
    /// When the commit was made.
    pub commit_timestamp: String,
}

/// About the build.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct BuildMetadata {
    /// When it was built.
    pub timestamp: String,
    /// Compiler version.
    pub rustc_version: String,
    /// Target platform.
    pub target_triple: String,
    /// Build profile.
    pub profile: String,
}

/// Everything the info endpoint returns.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AppInfoResponse {
    /// About the application.
    pub app: AppMetadata,
    /// About the commit.
    pub git: GitMetadata,
    /// About the build.
    pub build: BuildMetadata,
}

/// A logger's levels.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct LoggerLevelInfo {
    /// Level set for this logger.
    #[serde(rename = "configuredLevel")]
    pub configured_level: String,
    /// Level actually in effect.
    #[serde(rename = "effectiveLevel")]
    pub effective_level: String,
}

/// All loggers.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct LoggersResponse {
    /// Levels that can be set.
    pub levels: Vec<String>,
    /// Each logger and its levels.
    pub loggers: HashMap<String, LoggerLevelInfo>,
}

/// Request to change a logger's level.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct UpdateLoggerRequest {
    /// Level to set.
    #[serde(rename = "configuredLevel")]
    pub configured_level: String,
}

/// Result of changing a logger's level.
#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct UpdateLoggerResponse {
    /// The logger changed.
    pub name: String,
    /// Level now set.
    #[serde(rename = "configuredLevel")]
    pub configured_level: String,
}
