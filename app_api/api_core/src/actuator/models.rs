use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum HealthStatus {
    Up,
    Down,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct ComponentDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct ComponentHealth {
    pub status: HealthStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<ComponentDetails>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct HealthResponse {
    pub status: HealthStatus,
    pub components: HashMap<String, ComponentHealth>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct ProbeResponse {
    pub status: HealthStatus,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AppMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct GitMetadata {
    pub branch: String,
    pub commit_hash: String,
    pub commit_short_hash: String,
    pub commit_timestamp: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct BuildMetadata {
    pub timestamp: String,
    pub rustc_version: String,
    pub target_triple: String,
    pub profile: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct AppInfoResponse {
    pub app: AppMetadata,
    pub git: GitMetadata,
    pub build: BuildMetadata,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct LoggerLevelInfo {
    #[serde(rename = "configuredLevel")]
    pub configured_level: String,
    #[serde(rename = "effectiveLevel")]
    pub effective_level: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct LoggersResponse {
    pub levels: Vec<String>,
    pub loggers: HashMap<String, LoggerLevelInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct UpdateLoggerRequest {
    #[serde(rename = "configuredLevel")]
    pub configured_level: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, ToSchema)]
pub struct UpdateLoggerResponse {
    pub name: String,
    #[serde(rename = "configuredLevel")]
    pub configured_level: String,
}
