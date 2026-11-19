//! Turnover and cleaning crew dispatch domain rules and shared DTOs.
//!
//! Covers operational turnover statuses, role-enforced transition validation,
//! sanitized turnover board responses (zero guest PII or financials), and
//! staff roster / password-action payloads shared across microservices and
//! portals.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::{Validate, ValidateEmail, ValidationError};

/// Rejects invalid email addresses while tolerating leading/trailing whitespace
/// so copy-pasted addresses work cleanly after `.trim().to_lowercase()`.
fn validate_trimmed_email(email: &str) -> Result<(), ValidationError> {
    if is_valid_trimmed_email(email) {
        Ok(())
    } else {
        Err(ValidationError::new("email"))
    }
}

/// Returns `true` if `email.trim()` is a syntactically valid email address.
pub fn is_valid_trimmed_email(email: &str) -> bool {
    email.trim().validate_email()
}

/// Maximum length of a manual turnover title in characters.
pub const MAX_TURNOVER_TITLE_LEN: usize = 120;

/// Maximum length of a status transition reason note in characters.
pub const MAX_TURNOVER_REASON_LEN: usize = 500;

/// Maximum length of a turnover comment body in characters.
pub const MAX_TURNOVER_COMMENT_LEN: usize = 2000;

/// Operational lifecycle state of a turnover event.
///
/// Operational only: never blocks or alters guest reservation availability.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnoverStatus {
    /// Turnover awaits cleaning or triage.
    #[default]
    NeedsCleaning,
    /// Staff member has started turning the unit.
    CleaningInProgress,
    /// Cleaning finished; ready for host or admin inspection.
    CleaningCompleted,
    /// Repair or maintenance issue requires attention.
    MaintenanceRequired,
    /// Host or administrator signed off unit readiness.
    InspectedAndReady,
    /// Turnover cancelled or not needed (preserved with audit history).
    NotRequired,
}

impl TurnoverStatus {
    /// Every status in workflow display order.
    pub const ALL: [Self; 6] = [
        Self::NeedsCleaning,
        Self::CleaningInProgress,
        Self::CleaningCompleted,
        Self::MaintenanceRequired,
        Self::InspectedAndReady,
        Self::NotRequired,
    ];

    /// Snake-case identifier stored in PostgreSQL `CHECK` constraints and query params.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NeedsCleaning => "needs_cleaning",
            Self::CleaningInProgress => "cleaning_in_progress",
            Self::CleaningCompleted => "cleaning_completed",
            Self::MaintenanceRequired => "maintenance_required",
            Self::InspectedAndReady => "inspected_and_ready",
            Self::NotRequired => "not_required",
        }
    }

    /// Human-readable label shown in the dispatch UI and emails.
    pub const fn label(self) -> &'static str {
        match self {
            Self::NeedsCleaning => "Needs Cleaning",
            Self::CleaningInProgress => "Cleaning In Progress",
            Self::CleaningCompleted => "Cleaning Completed",
            Self::MaintenanceRequired => "Maintenance Required",
            Self::InspectedAndReady => "Inspected & Ready",
            Self::NotRequired => "Not Required",
        }
    }
}

impl fmt::Display for TurnoverStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TurnoverStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == normalized)
            .ok_or_else(|| format!("Unknown turnover status: {s}"))
    }
}

/// Origin that created a turnover record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnoverSourceKind {
    /// Eagerly derived from a native platform booking.
    Booking,
    /// Eagerly derived from an external iCal feed block.
    IcalFeed,
    /// Created manually for off-booking maintenance or manual blocks.
    Manual,
}

impl TurnoverSourceKind {
    /// Every source kind variant.
    pub const ALL: [Self; 3] = [Self::Booking, Self::IcalFeed, Self::Manual];

    /// Snake-case identifier persisted in `turnover.source_kind`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Booking => "booking",
            Self::IcalFeed => "ical_feed",
            Self::Manual => "manual",
        }
    }

    /// Human-readable badge label for dispatch cards.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Booking => "Booking",
            Self::IcalFeed => "iCal Feed",
            Self::Manual => "Manual",
        }
    }
}

impl fmt::Display for TurnoverSourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TurnoverSourceKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|kind| kind.as_str() == normalized)
            .ok_or_else(|| format!("Unknown turnover source kind: {s}"))
    }
}

/// Role acting on a turnover event or comment thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnoverActorRole {
    /// Platform administrator.
    Admin,
    /// Property owner / host.
    Host,
    /// Assigned cleaning or maintenance crew member.
    Staff,
}

impl TurnoverActorRole {
    /// Snake-case role identifier persisted in audit history and comments.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Host => "host",
            Self::Staff => "staff",
        }
    }
}

impl fmt::Display for TurnoverActorRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TurnoverActorRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "admin" => Ok(Self::Admin),
            "host" => Ok(Self::Host),
            "staff" => Ok(Self::Staff),
            other => Err(format!("Unknown turnover actor role: {other}")),
        }
    }
}

/// Date-window filter for the turnover dispatch board.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TurnoverWindow {
    /// Turnovers whose checkout date is today (Jamaica time).
    #[default]
    Today,
    /// Turnovers whose checkout date is strictly after today.
    Upcoming,
    /// Turnovers whose checkout date is strictly before today.
    Past,
    /// Turnovers across all dates.
    All,
}

impl TurnoverWindow {
    /// Query-string representation of the filter tab.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Today => "today",
            Self::Upcoming => "upcoming",
            Self::Past => "past",
            Self::All => "all",
        }
    }
}

impl fmt::Display for TurnoverWindow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TurnoverWindow {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "today" => Ok(Self::Today),
            "upcoming" => Ok(Self::Upcoming),
            "past" => Ok(Self::Past),
            "all" => Ok(Self::All),
            other => Err(format!("Unknown turnover window: {other}")),
        }
    }
}

/// Reason a requested turnover status transition was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error(
    "Role '{actor_role}' cannot transition {source_kind} turnover from {from} to {to}: {reason}"
)]
pub struct TurnoverTransitionError {
    /// Actor role that requested the change.
    pub actor_role: TurnoverActorRole,
    /// Source kind of the target turnover.
    pub source_kind: TurnoverSourceKind,
    /// Current turnover status.
    pub from: TurnoverStatus,
    /// Requested destination status.
    pub to: TurnoverStatus,
    /// Stable explanation of why the rule refused the transition.
    pub reason: &'static str,
}

impl TurnoverTransitionError {
    /// Machine-readable error code returned by HTTP APIs.
    pub const fn code(&self) -> &'static str {
        "TURNOVER_TRANSITION_FORBIDDEN"
    }
}

/// Validates whether `actor_role` may transition a `source_kind` turnover from `from` to `to`.
///
/// Rules:
/// - No-op (`from == to`) is permitted for every actor.
/// - `Admin` and `Host` may transition between any two statuses across every source kind.
/// - `Staff` cannot transition into `InspectedAndReady` from any status.
/// - `Staff` CAN transition out of `InspectedAndReady` (e.g., to report post-inspection maintenance).
/// - `Staff` can transition to `NotRequired` only when `source_kind == TurnoverSourceKind::Manual`.
/// - From `NotRequired`, `Staff` may reopen only to `NeedsCleaning` or `MaintenanceRequired`.
pub fn can_transition(
    actor_role: TurnoverActorRole,
    source_kind: TurnoverSourceKind,
    from: TurnoverStatus,
    to: TurnoverStatus,
) -> Result<(), TurnoverTransitionError> {
    if from == to {
        return Ok(());
    }

    if matches!(
        actor_role,
        TurnoverActorRole::Admin | TurnoverActorRole::Host
    ) {
        return Ok(());
    }

    if to == TurnoverStatus::InspectedAndReady {
        return Err(TurnoverTransitionError {
            actor_role,
            source_kind,
            from,
            to,
            reason: "Only hosts or administrators can mark a turnover Inspected & Ready",
        });
    }

    if to == TurnoverStatus::NotRequired && source_kind != TurnoverSourceKind::Manual {
        return Err(TurnoverTransitionError {
            actor_role,
            source_kind,
            from,
            to,
            reason: "Only hosts or administrators can mark booking or iCal turnovers Not Required",
        });
    }

    if from == TurnoverStatus::NotRequired
        && !matches!(
            to,
            TurnoverStatus::NeedsCleaning | TurnoverStatus::MaintenanceRequired
        )
    {
        return Err(TurnoverTransitionError {
            actor_role,
            source_kind,
            from,
            to,
            reason: "Staff can reopen Not Required turnovers only to Needs Cleaning or Maintenance Required",
        });
    }

    Ok(())
}

/// Returns all destination statuses `actor_role` can transition to from `current`.
pub fn allowed_next_statuses(
    actor_role: TurnoverActorRole,
    source_kind: TurnoverSourceKind,
    current: TurnoverStatus,
) -> Vec<TurnoverStatus> {
    TurnoverStatus::ALL
        .into_iter()
        .filter(|next| {
            *next != current && can_transition(actor_role, source_kind, current, *next).is_ok()
        })
        .collect()
}

/// Operational summary of a turnover event (zero guest PII or financials).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TurnoverResponse {
    /// Unique turnover identifier.
    pub id: Uuid,
    /// Property listing identifier.
    pub listing_id: Uuid,
    /// Property listing display name.
    pub listing_name: String,
    /// City/parish label for quick dispatch context.
    pub listing_city: Option<String>,
    /// House-policy check-in time label if configured.
    pub checkin_policy_text: Option<String>,
    /// House-policy check-out time label if configured.
    pub checkout_policy_text: Option<String>,
    /// Whether created from a booking, external iCal feed, or manually.
    pub source_kind: TurnoverSourceKind,
    /// Checkout / scheduled turnover date.
    pub checkout_date: NaiveDate,
    /// Dynamically computed earliest confirmed stay or calendar block starting on or after `checkout_date`.
    pub next_checkin_date: Option<NaiveDate>,
    /// Current operational readiness status.
    pub status: TurnoverStatus,
    /// Optional title/label (commonly supplied on manual maintenance turnovers).
    pub title: Option<String>,
    /// Display names of active staff currently assigned to this listing.
    pub assigned_staff_names: Vec<String>,
    /// Total count of internal crew comments posted on this turnover.
    pub comment_count: i64,
    /// Timestamp of most recent turnover update.
    pub updated_at: DateTime<Utc>,
}

/// Immutable audit row recording one turnover status transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TurnoverStatusHistoryEntry {
    /// Unique history entry identifier.
    pub id: Uuid,
    /// Target turnover identifier.
    pub turnover_id: Uuid,
    /// Prior status (`None` when initially created).
    pub from_status: Option<TurnoverStatus>,
    /// Resulting status.
    pub to_status: TurnoverStatus,
    /// User identifier of the actor, if human-initiated and still present.
    pub actor_user_id: Option<Uuid>,
    /// Actor display name when resolvable.
    pub actor_name: Option<String>,
    /// Actor role label (`admin`, `host`, `staff`, or `system`).
    pub actor_role: String,
    /// System entrypoint or trigger name (e.g., `dispatch_board`, `booking_update`).
    pub entry_point: String,
    /// Optional explanation provided for the transition.
    pub reason: Option<String>,
    /// Timestamp when the transition occurred.
    pub created_at: DateTime<Utc>,
}

/// Single flat chronological comment on a turnover thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TurnoverCommentResponse {
    /// Unique comment identifier.
    pub id: Uuid,
    /// Target turnover identifier.
    pub turnover_id: Uuid,
    /// Comment author's user ID (`None` if the author's user record was purged).
    pub author_user_id: Option<Uuid>,
    /// Author name snapshot captured at creation time.
    pub author_name: String,
    /// Author role (`host`, `admin`, or `staff`).
    pub author_role: String,
    /// Plain-text comment body (rendered escaped in HTML and emails).
    pub body: String,
    /// Timestamp when the comment was posted.
    pub created_at: DateTime<Utc>,
}

/// Detailed view of a turnover including audit history and comment thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TurnoverDetailResponse {
    /// Core turnover summary.
    pub turnover: TurnoverResponse,
    /// Chronological status audit trail.
    pub status_history: Vec<TurnoverStatusHistoryEntry>,
    /// Flat chronological comment log.
    pub comments: Vec<TurnoverCommentResponse>,
}

/// Request payload to create a manual turnover or maintenance event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, ToSchema)]
pub struct CreateManualTurnoverRequest {
    /// Listing receiving the manual turnover.
    pub listing_id: Uuid,
    /// Target checkout / maintenance date.
    pub checkout_date: NaiveDate,
    /// Optional descriptive title (max 120 characters).
    #[serde(default)]
    #[validate(length(max = 120))]
    pub title: Option<String>,
    /// Optional initial operational status (defaults to `needs_cleaning`).
    #[serde(default)]
    pub initial_status: Option<TurnoverStatus>,
}

/// Request payload to transition a turnover's operational status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, ToSchema)]
pub struct TransitionTurnoverRequest {
    /// Target operational status.
    pub to_status: TurnoverStatus,
    /// Optional optimistic-concurrency guard against concurrent status updates.
    #[serde(default)]
    pub expected_status: Option<TurnoverStatus>,
    /// Optional reason/note attached to the status history entry (max 500 characters).
    #[serde(default)]
    #[validate(length(max = 500))]
    pub reason: Option<String>,
}

/// Request payload to post a comment on a turnover thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, ToSchema)]
pub struct CreateTurnoverCommentRequest {
    /// Comment message body (1 to 2000 characters).
    #[validate(length(min = 1, max = 2000))]
    pub body: String,
}

/// Query parameters accepted by `GET /api/v1/turnovers`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct TurnoverListQuery {
    /// Date bucket filter (`today`, `upcoming`, `past`, `all`).
    #[serde(default)]
    pub window: Option<TurnoverWindow>,
    /// Optional exact status filter.
    #[serde(default)]
    pub status: Option<TurnoverStatus>,
    /// Optional single-listing scope filter.
    #[serde(default)]
    pub listing_id: Option<Uuid>,
    /// 1-based page number (defaults to 1).
    #[serde(default)]
    pub page: Option<u32>,
    /// Page size capped server-side (defaults to 20, max 100).
    #[serde(default)]
    pub per_page: Option<u32>,
}

/// Paginated turnover dispatch board response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TurnoverListResponse {
    /// Turnovers matching filters within the caller's scope.
    pub turnovers: Vec<TurnoverResponse>,
    /// Total matching records before pagination.
    pub total: i64,
    /// Current 1-based page number.
    pub page: u32,
    /// Effective page size.
    pub per_page: u32,
}

/// Listing assignment attached to a staff member within a host's scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StaffListingAssignment {
    /// Assigned listing identifier.
    pub listing_id: Uuid,
    /// Listing title.
    pub listing_name: String,
    /// Timestamp when this listing was assigned.
    pub assigned_at: DateTime<Utc>,
}

/// Host-scoped staff roster entry returned by `/api/v1/staff-users`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StaffMemberSummary {
    /// Staff member's user ID.
    pub user_id: Uuid,
    /// Email address.
    pub email: String,
    /// First name.
    pub first_name: String,
    /// Last name.
    pub last_name: String,
    /// Whether the underlying account is active.
    pub is_active: bool,
    /// Whether the email/account has completed verification or password setup.
    pub is_verified: bool,
    /// True if the account still has an unusable sentinel password awaiting setup.
    pub password_pending: bool,
    /// Owned listings (or all assigned listings for admin) that this staff member can dispatch.
    pub listings: Vec<StaffListingAssignment>,
    /// Timestamp when added to this host's roster.
    pub invited_at: DateTime<Utc>,
}

/// Query parameters for `GET /api/v1/staff-users`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, IntoParams, ToSchema)]
pub struct StaffListQuery {
    /// Optional case-insensitive search against name or email.
    #[serde(default)]
    pub q: Option<String>,
    /// 1-based page number (defaults to 1).
    #[serde(default)]
    pub page: Option<u32>,
    /// Page size capped server-side (defaults to 20, max 100).
    #[serde(default)]
    pub per_page: Option<u32>,
}

/// Paginated roster list response for `/api/v1/staff-users`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StaffListResponse {
    /// Staff members linked to the host (or platform-wide for admin).
    pub staff: Vec<StaffMemberSummary>,
    /// Total matching staff rows before pagination.
    pub total: i64,
    /// Current 1-based page number.
    pub page: u32,
    /// Effective page size.
    pub per_page: u32,
}

/// Request payload to invite or assign a cleaning/maintenance staff member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, ToSchema)]
pub struct InviteStaffRequest {
    /// Staff email address.
    #[validate(custom(function = "validate_trimmed_email"))]
    pub email: String,
    /// First name used only when creating a brand-new account.
    #[serde(default)]
    pub first_name: Option<String>,
    /// Last name used only when creating a brand-new account.
    #[serde(default)]
    pub last_name: Option<String>,
    /// One or more listing IDs owned by the caller to assign immediately.
    #[validate(length(min = 1))]
    pub listing_ids: Vec<Uuid>,
}

/// Uniform non-enumerating response returned by `POST /api/v1/staff-users`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct StaffInviteResponse {
    /// Normalized email address submitted by the host.
    pub email: String,
    /// Generic invitation dispatch message.
    pub message: String,
}

/// Public request payload to accept an existing-account staff roster invitation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, ToSchema)]
pub struct StaffInviteAcceptRequest {
    /// Raw opaque `staff_accept` token parsed from the email link query string.
    #[validate(length(min = 1))]
    pub token: String,
}

/// Request payload to replace a staff member's assigned listings for the caller's properties.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UpdateStaffAssignmentsRequest {
    /// Replacement set of caller-owned listing IDs.
    pub listing_ids: Vec<Uuid>,
}

/// Public request payload to initiate a password-reset email link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, ToSchema)]
pub struct PasswordResetRequest {
    /// Target account email address.
    #[validate(custom(function = "validate_trimmed_email"))]
    pub email: String,
}

/// Public request payload to redeem an invite or password-reset token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate, ToSchema)]
pub struct PasswordTokenRedeemRequest {
    /// Raw opaque token parsed from the email link query string.
    #[validate(length(min = 1))]
    pub token: String,
    /// Chosen new password (minimum 8 characters).
    #[validate(length(min = 8))]
    pub new_password: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turnover_enum_roundtrips() -> Result<(), Box<dyn std::error::Error>> {
        for status in TurnoverStatus::ALL {
            let s = status.to_string();
            let parsed = TurnoverStatus::from_str(&s)?;
            assert_eq!(parsed, status);
            assert!(!status.label().is_empty());
        }
        assert!(TurnoverStatus::from_str("invalid_status").is_err());

        for kind in TurnoverSourceKind::ALL {
            let s = kind.to_string();
            assert_eq!(TurnoverSourceKind::from_str(&s)?, kind);
            assert!(!kind.label().is_empty());
        }
        assert!(TurnoverSourceKind::from_str("other").is_err());

        for role in [
            TurnoverActorRole::Admin,
            TurnoverActorRole::Host,
            TurnoverActorRole::Staff,
        ] {
            let s = role.to_string();
            assert_eq!(TurnoverActorRole::from_str(&s)?, role);
        }
        assert!(TurnoverActorRole::from_str("guest").is_err());

        for window in [
            TurnoverWindow::Today,
            TurnoverWindow::Upcoming,
            TurnoverWindow::Past,
            TurnoverWindow::All,
        ] {
            let s = window.to_string();
            assert_eq!(TurnoverWindow::from_str(&s)?, window);
        }
        assert!(TurnoverWindow::from_str("next_week").is_err());
        Ok(())
    }

    #[test]
    fn host_and_admin_can_transition_between_all_statuses() {
        for role in [TurnoverActorRole::Admin, TurnoverActorRole::Host] {
            for kind in TurnoverSourceKind::ALL {
                for from in TurnoverStatus::ALL {
                    for to in TurnoverStatus::ALL {
                        assert!(
                            can_transition(role, kind, from, to).is_ok(),
                            "{role:?} should transition {kind:?} {from:?} -> {to:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn staff_transition_matrix_enforces_inspection_and_not_required_rules() {
        for kind in TurnoverSourceKind::ALL {
            for from in TurnoverStatus::ALL {
                for to in TurnoverStatus::ALL {
                    let result = can_transition(TurnoverActorRole::Staff, kind, from, to);

                    let expected_ok = if from == to {
                        true
                    } else if to == TurnoverStatus::InspectedAndReady {
                        false
                    } else if to == TurnoverStatus::NotRequired {
                        kind == TurnoverSourceKind::Manual
                    } else if from == TurnoverStatus::NotRequired {
                        matches!(
                            to,
                            TurnoverStatus::NeedsCleaning | TurnoverStatus::MaintenanceRequired
                        )
                    } else {
                        true
                    };

                    assert_eq!(
                        result.is_ok(),
                        expected_ok,
                        "mismatch for Staff {kind:?}: {from:?} -> {to:?}: {result:?}"
                    );

                    if let Err(err) = result {
                        assert_eq!(err.code(), "TURNOVER_TRANSITION_FORBIDDEN");
                    }
                }
            }
        }
    }

    #[test]
    fn allowed_next_statuses_excludes_current_and_forbidden_targets() {
        let next_booking_staff = allowed_next_statuses(
            TurnoverActorRole::Staff,
            TurnoverSourceKind::Booking,
            TurnoverStatus::CleaningCompleted,
        );
        assert!(!next_booking_staff.contains(&TurnoverStatus::CleaningCompleted));
        assert!(!next_booking_staff.contains(&TurnoverStatus::InspectedAndReady));
        assert!(!next_booking_staff.contains(&TurnoverStatus::NotRequired));
        assert!(next_booking_staff.contains(&TurnoverStatus::MaintenanceRequired));

        let next_inspected_staff = allowed_next_statuses(
            TurnoverActorRole::Staff,
            TurnoverSourceKind::Booking,
            TurnoverStatus::InspectedAndReady,
        );
        assert!(next_inspected_staff.contains(&TurnoverStatus::MaintenanceRequired));
        assert!(next_inspected_staff.contains(&TurnoverStatus::NeedsCleaning));
        assert!(!next_inspected_staff.contains(&TurnoverStatus::InspectedAndReady));

        let next_manual_staff = allowed_next_statuses(
            TurnoverActorRole::Staff,
            TurnoverSourceKind::Manual,
            TurnoverStatus::NeedsCleaning,
        );
        assert!(next_manual_staff.contains(&TurnoverStatus::NotRequired));
    }
}
