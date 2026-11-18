//! Host-scoped staff roster, listing assignments, and invite onboarding persistence.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use common::email::{EmailTemplate, StaffInviteEmailPayload, StaffListingAssignedEmailPayload};
use common::turnover::{
    InviteStaffRequest, StaffListQuery, StaffListResponse, StaffListingAssignment,
    StaffMemberSummary,
};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::action_token::{
    ActionTokenPurpose, is_password_pending, issue_action_token_conn, locked_password_sentinel,
};
use crate::email_outbox::insert_email_outbox_conn;
use crate::error::{DbError, Result};
use crate::models::{NewUser, UserRole};
use crate::sessions::SessionsDb;
use crate::user::{create_user, lock_and_snapshot};

/// Result of inviting or attaching a cleaning/maintenance staff member.
#[derive(Debug, Clone)]
pub struct StaffInviteOutcome {
    /// Scoped staff summary visible to the invoking host/admin.
    pub staff: StaffMemberSummary,
    /// True if a new user or host-staff relationship was created (`201 Created` vs `200 OK`).
    pub created: bool,
    /// Queued email outbox row ID to notify `email_worker` asynchronously after commit.
    pub outbox_email_id: Option<Uuid>,
}

/// Escapes PostgreSQL `LIKE` / `ILIKE` special wildcard characters (`\`, `%`, `_`)
/// so user search terms match literally with `ESCAPE '\'`.
pub fn escape_like_pattern(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '\\' | '%' | '_' => {
                out.push('\\');
                out.push(ch);
            }
            other => out.push(other),
        }
    }
    out
}

#[derive(Debug)]
struct StaffRow {
    id: Uuid,
    email: String,
    first_name: String,
    last_name: String,
    password_hash: String,
    is_active: bool,
    is_verified: bool,
    invited_at: DateTime<Utc>,
}

#[derive(Debug)]
struct LockedListing {
    id: Uuid,
    owner_id: Uuid,
    name: String,
}

async fn set_lock_timeout(conn: &mut PgConnection) -> Result<()> {
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *conn)
        .await
        .map_err(|e| DbError::from_sqlx_contended(e, "staff set lock_timeout"))?;
    Ok(())
}

async fn lock_email_advisory(conn: &mut PgConnection, normalized_email: &str) -> Result<()> {
    let _locked = sqlx::query_scalar!(
        r#"SELECT pg_advisory_xact_lock(hashtext('staff_invite'), hashtext($1)) IS NULL AS "locked!""#,
        normalized_email
    )
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "lock staff invite email"))?;
    Ok(())
}

fn normalized_display_name(first_name: &str, last_name: &str, fallback_email: &str) -> String {
    let joined = format!("{first_name} {last_name}").trim().to_string();
    if joined.is_empty() {
        fallback_email.to_string()
    } else {
        joined
    }
}

async fn load_caller_display_name(conn: &mut PgConnection, caller_id: Uuid) -> Result<String> {
    let row = sqlx::query!(
        r#"SELECT first_name, last_name, email FROM "user" WHERE id = $1"#,
        caller_id
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(normalized_display_name(
        &row.first_name,
        &row.last_name,
        &row.email,
    ))
}

/// Validates and locks the requested listing IDs in ascending ID order.
async fn validate_and_lock_listings(
    conn: &mut PgConnection,
    caller_id: Uuid,
    caller_is_admin: bool,
    raw_listing_ids: &[Uuid],
) -> Result<Vec<LockedListing>> {
    let mut unique_ids = raw_listing_ids.to_vec();
    unique_ids.sort_unstable();
    unique_ids.dedup();
    if unique_ids.is_empty() {
        return Err(DbError::ValidationError(
            "Select at least one active listing.".to_string(),
        ));
    }

    let rows = sqlx::query!(
        r#"
        SELECT id, user_id, name, deleted_at
        FROM listing
        WHERE id = ANY($1)
        ORDER BY id
        FOR UPDATE
        "#,
        &unique_ids,
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "lock target listings for staff"))?;

    if rows.len() != unique_ids.len() || rows.iter().any(|r| r.deleted_at.is_some()) {
        return Err(DbError::ValidationError(
            "One or more selected listings do not exist or have been deleted.".to_string(),
        ));
    }

    if !caller_is_admin && rows.iter().any(|r| r.user_id != caller_id) {
        return Err(DbError::Forbidden(
            "You do not own one or more selected listings.".to_string(),
        ));
    }

    Ok(rows
        .into_iter()
        .map(|r| LockedListing {
            id: r.id,
            owner_id: r.user_id,
            name: r.name,
        })
        .collect())
}

async fn attach_assignments_for_rows(
    conn: &mut PgConnection,
    caller_id: Uuid,
    caller_is_admin: bool,
    rows: Vec<StaffRow>,
) -> Result<Vec<StaffMemberSummary>> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let staff_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let assignment_rows = sqlx::query!(
        r#"
        SELECT ls.staff_user_id, ls.listing_id, l.name AS listing_name, ls.created_at AS assigned_at
        FROM listing_staff ls
        INNER JOIN listing l ON l.id = ls.listing_id
        WHERE ls.staff_user_id = ANY($1)
          AND l.deleted_at IS NULL
          AND ($2 OR l.user_id = $3)
        ORDER BY l.name ASC, ls.listing_id ASC
        "#,
        &staff_ids,
        caller_is_admin,
        caller_id,
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut by_staff: BTreeMap<Uuid, Vec<StaffListingAssignment>> = BTreeMap::new();
    for row in assignment_rows {
        by_staff
            .entry(row.staff_user_id)
            .or_default()
            .push(StaffListingAssignment {
                listing_id: row.listing_id,
                listing_name: row.listing_name,
                assigned_at: row.assigned_at,
            });
    }

    Ok(rows
        .into_iter()
        .map(|row| StaffMemberSummary {
            user_id: row.id,
            email: row.email,
            first_name: row.first_name,
            last_name: row.last_name,
            is_active: row.is_active,
            is_verified: row.is_verified,
            password_pending: is_password_pending(&row.password_hash),
            listings: by_staff.remove(&row.id).unwrap_or_default(),
            invited_at: row.invited_at,
        })
        .collect())
}

async fn load_scoped_staff_member(
    conn: &mut PgConnection,
    caller_id: Uuid,
    caller_is_admin: bool,
    staff_user_id: Uuid,
) -> Result<StaffMemberSummary> {
    let row = if caller_is_admin {
        sqlx::query_as!(
            StaffRow,
            r#"
            SELECT
                u.id,
                u.email,
                u.first_name,
                u.last_name,
                u.password_hash,
                u.is_active,
                u.is_verified,
                COALESCE(
                    (SELECT MIN(hs.created_at) FROM host_staff hs WHERE hs.staff_user_id = u.id),
                    u.created_at
                ) AS "invited_at!"
            FROM "user" u
            WHERE u.id = $1
              AND u.deleted_at IS NULL
              AND (
                  'staff'::user_role = ANY(u.roles)
                  OR EXISTS (SELECT 1 FROM host_staff hs WHERE hs.staff_user_id = u.id)
              )
            "#,
            staff_user_id,
        )
        .fetch_optional(&mut *conn)
        .await?
    } else {
        sqlx::query_as!(
            StaffRow,
            r#"
            SELECT
                u.id,
                u.email,
                u.first_name,
                u.last_name,
                u.password_hash,
                u.is_active,
                u.is_verified,
                hs.created_at AS invited_at
            FROM host_staff hs
            INNER JOIN "user" u ON u.id = hs.staff_user_id
            WHERE hs.host_user_id = $1
              AND u.id = $2
              AND u.deleted_at IS NULL
            "#,
            caller_id,
            staff_user_id,
        )
        .fetch_optional(&mut *conn)
        .await?
    }
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    let mut list = attach_assignments_for_rows(conn, caller_id, caller_is_admin, vec![row]).await?;
    list.pop().ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))
}

/// Lists cleaning/maintenance staff members within the caller's scope (`host_staff` for hosts,
/// platform-wide for administrators), escaping SQL `LIKE`/`ILIKE` wildcards in `query.q`.
#[tracing::instrument(skip(pool))]
pub async fn list_staff_users(
    pool: &PgPool,
    caller_id: Uuid,
    caller_is_admin: bool,
    query: &StaffListQuery,
) -> Result<StaffListResponse> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);
    let offset = i64::from(page.saturating_sub(1)) * i64::from(per_page);
    let limit = i64::from(per_page);
    let pattern = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{}%", escape_like_pattern(s)));

    let mut conn = pool.acquire().await?;
    let (total, rows) = if caller_is_admin {
        let total = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM "user" u
            WHERE u.deleted_at IS NULL
              AND (
                  'staff'::user_role = ANY(u.roles)
                  OR EXISTS (SELECT 1 FROM host_staff hs WHERE hs.staff_user_id = u.id)
              )
              AND (
                  $1::text IS NULL
                  OR u.email ILIKE $1 ESCAPE '\'
                  OR u.first_name ILIKE $1 ESCAPE '\'
                  OR u.last_name ILIKE $1 ESCAPE '\'
                  OR (u.first_name || ' ' || u.last_name) ILIKE $1 ESCAPE '\'
              )
            "#,
            pattern,
        )
        .fetch_one(&mut *conn)
        .await?;

        let rows = sqlx::query_as!(
            StaffRow,
            r#"
            SELECT
                u.id,
                u.email,
                u.first_name,
                u.last_name,
                u.password_hash,
                u.is_active,
                u.is_verified,
                COALESCE(
                    (SELECT MIN(hs.created_at) FROM host_staff hs WHERE hs.staff_user_id = u.id),
                    u.created_at
                ) AS "invited_at!"
            FROM "user" u
            WHERE u.deleted_at IS NULL
              AND (
                  'staff'::user_role = ANY(u.roles)
                  OR EXISTS (SELECT 1 FROM host_staff hs WHERE hs.staff_user_id = u.id)
              )
              AND (
                  $1::text IS NULL
                  OR u.email ILIKE $1 ESCAPE '\'
                  OR u.first_name ILIKE $1 ESCAPE '\'
                  OR u.last_name ILIKE $1 ESCAPE '\'
                  OR (u.first_name || ' ' || u.last_name) ILIKE $1 ESCAPE '\'
              )
            ORDER BY "invited_at!" DESC, u.id DESC
            LIMIT $2 OFFSET $3
            "#,
            pattern,
            limit,
            offset,
        )
        .fetch_all(&mut *conn)
        .await?;
        (total, rows)
    } else {
        let total = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM host_staff hs
            INNER JOIN "user" u ON u.id = hs.staff_user_id
            WHERE hs.host_user_id = $1
              AND u.deleted_at IS NULL
              AND (
                  $2::text IS NULL
                  OR u.email ILIKE $2 ESCAPE '\'
                  OR u.first_name ILIKE $2 ESCAPE '\'
                  OR u.last_name ILIKE $2 ESCAPE '\'
                  OR (u.first_name || ' ' || u.last_name) ILIKE $2 ESCAPE '\'
              )
            "#,
            caller_id,
            pattern,
        )
        .fetch_one(&mut *conn)
        .await?;

        let rows = sqlx::query_as!(
            StaffRow,
            r#"
            SELECT
                u.id,
                u.email,
                u.first_name,
                u.last_name,
                u.password_hash,
                u.is_active,
                u.is_verified,
                hs.created_at AS invited_at
            FROM host_staff hs
            INNER JOIN "user" u ON u.id = hs.staff_user_id
            WHERE hs.host_user_id = $1
              AND u.deleted_at IS NULL
              AND (
                  $2::text IS NULL
                  OR u.email ILIKE $2 ESCAPE '\'
                  OR u.first_name ILIKE $2 ESCAPE '\'
                  OR u.last_name ILIKE $2 ESCAPE '\'
                  OR (u.first_name || ' ' || u.last_name) ILIKE $2 ESCAPE '\'
              )
            ORDER BY hs.created_at DESC, u.id DESC
            LIMIT $3 OFFSET $4
            "#,
            caller_id,
            pattern,
            limit,
            offset,
        )
        .fetch_all(&mut *conn)
        .await?;
        (total, rows)
    };

    let staff = attach_assignments_for_rows(&mut conn, caller_id, caller_is_admin, rows).await?;
    Ok(StaffListResponse {
        staff,
        total,
        page,
        per_page,
    })
}

/// Atomically invites a new staff account (or attaches an existing active user to the caller's
/// roster and listings), queuing either a password-setup invite email or listing-assignment email.
#[tracing::instrument(skip(pool, req))]
pub async fn invite_or_attach_staff(
    pool: &PgPool,
    caller_id: Uuid,
    caller_is_admin: bool,
    req: &InviteStaffRequest,
    admin_base_url: &str,
) -> Result<StaffInviteOutcome> {
    let normalized_email = req.email.trim().to_lowercase();
    if normalized_email.is_empty() {
        return Err(DbError::ValidationError(
            "Email address is required.".to_string(),
        ));
    }
    if req.listing_ids.is_empty() {
        return Err(DbError::ValidationError(
            "Select at least one active listing.".to_string(),
        ));
    }

    let mut tx = pool.begin().await?;
    set_lock_timeout(&mut tx).await?;
    lock_email_advisory(&mut tx, &normalized_email).await?;

    let locked_listings =
        validate_and_lock_listings(&mut tx, caller_id, caller_is_admin, &req.listing_ids).await?;

    let existing_user = sqlx::query!(
        r#"
        SELECT
            id,
            email,
            password_hash,
            first_name,
            is_active,
            deleted_at,
            roles AS "roles: Vec<UserRole>"
        FROM "user"
        WHERE LOWER(BTRIM(email)) = $1
        ORDER BY created_at ASC
        LIMIT 1
        FOR UPDATE
        "#,
        normalized_email,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "lock target user for staff invite"))?;

    let (staff_user_id, recipient_email, staff_first_name, pending_password, created_user) =
        if let Some(u) = existing_user {
            if !u.is_active || u.deleted_at.is_some() {
                return Err(DbError::ValidationError(
                    "An inactive or deleted account already exists for this email.".to_string(),
                ));
            }
            if !u.roles.contains(&UserRole::Staff) {
                lock_and_snapshot(&mut tx, u.id).await?;
                sqlx::query!(
                    r#"
                    UPDATE "user"
                    SET roles = array_append(roles, 'staff'::user_role),
                        updated_at = NOW()
                    WHERE id = $1
                    "#,
                    u.id,
                )
                .execute(&mut *tx)
                .await?;
            }
            (
                u.id,
                u.email,
                u.first_name,
                is_password_pending(&u.password_hash),
                false,
            )
        } else {
            let first_name = req
                .first_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("Staff")
                .to_string();
            let last_name = req
                .last_name
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .to_string();
            let new_user = NewUser {
                id: Uuid::now_v7(),
                email: normalized_email.clone(),
                password_hash: locked_password_sentinel(),
                first_name: first_name.clone(),
                last_name,
                phone_number: None,
                is_active: true,
                is_verified: false,
                verification_code: None,
                verification_code_expires_at: None,
                attributes: serde_json::json!({}),
                roles: Some(vec![UserRole::Staff]),
                default_currency: "USD".to_string(),
            };
            let inserted = create_user(&mut *tx, &new_user).await?;
            (inserted.id, inserted.email, inserted.first_name, true, true)
        };

    let target_hosts: Vec<Uuid> = if caller_is_admin {
        let mut set = BTreeSet::new();
        for l in &locked_listings {
            set.insert(l.owner_id);
        }
        set.into_iter().collect()
    } else {
        vec![caller_id]
    };

    let mut created_host_link = false;
    for host_id in target_hosts {
        let inserted_host_link = sqlx::query_scalar!(
            r#"
            INSERT INTO host_staff (host_user_id, staff_user_id, invited_by)
            VALUES ($1, $2, $3)
            ON CONFLICT (host_user_id, staff_user_id) DO NOTHING
            RETURNING id
            "#,
            host_id,
            staff_user_id,
            caller_id,
        )
        .fetch_optional(&mut *tx)
        .await?;
        if inserted_host_link.is_some() {
            created_host_link = true;
        }
    }

    let mut newly_assigned_names = Vec::new();
    let mut all_requested_names = Vec::with_capacity(locked_listings.len());
    for listing in &locked_listings {
        all_requested_names.push(listing.name.clone());
        let inserted_assignment = sqlx::query_scalar!(
            r#"
            INSERT INTO listing_staff (listing_id, staff_user_id, assigned_by)
            VALUES ($1, $2, $3)
            ON CONFLICT (listing_id, staff_user_id) DO NOTHING
            RETURNING listing_id
            "#,
            listing.id,
            staff_user_id,
            caller_id,
        )
        .fetch_optional(&mut *tx)
        .await?;
        if inserted_assignment.is_some() {
            newly_assigned_names.push(listing.name.clone());
        }
    }
    all_requested_names.sort();
    newly_assigned_names.sort();

    let host_name = load_caller_display_name(&mut tx, caller_id).await?;
    let base_url = admin_base_url.trim_end_matches('/');

    let outbox_email_id = if pending_password {
        let expires_at = Utc::now() + ActionTokenPurpose::StaffInvite.default_ttl();
        let raw_token = issue_action_token_conn(
            &mut tx,
            staff_user_id,
            ActionTokenPurpose::StaffInvite,
            expires_at,
        )
        .await?;
        let setup_url = format!("{base_url}/reset-password?token={raw_token}");
        let payload = serde_json::to_value(StaffInviteEmailPayload {
            first_name: staff_first_name,
            host_name,
            listing_names: all_requested_names,
            setup_url,
            expires_hours: 72,
        })
        .map_err(|e| DbError::ValidationError(format!("invalid invite email payload: {e}")))?;
        let outbox = insert_email_outbox_conn(
            &mut tx,
            &recipient_email,
            "You've been invited to Our Places Turnover Dispatch",
            EmailTemplate::StaffInvite.as_str(),
            &payload,
            3,
        )
        .await?;
        Some(outbox.id)
    } else if created_host_link || !newly_assigned_names.is_empty() {
        let listing_names = if newly_assigned_names.is_empty() {
            all_requested_names
        } else {
            newly_assigned_names
        };
        let payload = serde_json::to_value(StaffListingAssignedEmailPayload {
            first_name: staff_first_name,
            host_name,
            listing_names,
            dispatch_url: format!("{base_url}/admin/turnovers"),
        })
        .map_err(|e| DbError::ValidationError(format!("invalid assignment email payload: {e}")))?;
        let outbox = insert_email_outbox_conn(
            &mut tx,
            &recipient_email,
            "New turnover listings assigned on Our Places",
            EmailTemplate::StaffListingAssigned.as_str(),
            &payload,
            3,
        )
        .await?;
        Some(outbox.id)
    } else {
        None
    };

    let summary =
        load_scoped_staff_member(&mut tx, caller_id, caller_is_admin, staff_user_id).await?;
    tx.commit().await?;

    Ok(StaffInviteOutcome {
        staff: summary,
        created: created_user || created_host_link,
        outbox_email_id,
    })
}

/// Replaces a staff member's assigned listings within the caller's ownership scope
/// and queues `StaffListingAssigned` for newly added properties if password setup is complete.
#[tracing::instrument(skip(pool, listing_ids))]
pub async fn update_staff_listings(
    pool: &PgPool,
    caller_id: Uuid,
    caller_is_admin: bool,
    staff_user_id: Uuid,
    listing_ids: &[Uuid],
    admin_base_url: &str,
) -> Result<(StaffMemberSummary, Option<Uuid>)> {
    if listing_ids.is_empty() {
        return Err(DbError::ValidationError(
            "Select at least one active listing, or remove the staff member from your roster."
                .to_string(),
        ));
    }

    let mut tx = pool.begin().await?;
    set_lock_timeout(&mut tx).await?;

    let target = sqlx::query!(
        r#"
        SELECT id, email, first_name, password_hash, is_active, deleted_at, roles AS "roles: Vec<UserRole>"
        FROM "user"
        WHERE id = $1
          AND deleted_at IS NULL
        FOR UPDATE
        "#,
        staff_user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "lock staff user for assignment update"))?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    if !target.is_active {
        return Err(DbError::ValidationError(
            "Cannot assign listings to an inactive user account.".to_string(),
        ));
    }

    if !caller_is_admin {
        let in_roster = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM host_staff WHERE host_user_id = $1 AND staff_user_id = $2
            ) AS "exists!"
            "#,
            caller_id,
            staff_user_id,
        )
        .fetch_one(&mut *tx)
        .await?;
        if !in_roster {
            return Err(DbError::Sqlx(sqlx::Error::RowNotFound));
        }
    } else if !target.roles.contains(&UserRole::Staff) {
        let has_host_link = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM host_staff WHERE staff_user_id = $1) AS "exists!""#,
            staff_user_id,
        )
        .fetch_one(&mut *tx)
        .await?;
        if !has_host_link {
            return Err(DbError::Sqlx(sqlx::Error::RowNotFound));
        }
    }

    let locked_listings =
        validate_and_lock_listings(&mut tx, caller_id, caller_is_admin, listing_ids).await?;

    if caller_is_admin {
        let mut owners = BTreeSet::new();
        for l in &locked_listings {
            owners.insert(l.owner_id);
        }
        for owner_id in owners {
            sqlx::query!(
                r#"
                INSERT INTO host_staff (host_user_id, staff_user_id, invited_by)
                VALUES ($1, $2, $3)
                ON CONFLICT (host_user_id, staff_user_id) DO NOTHING
                "#,
                owner_id,
                staff_user_id,
                caller_id,
            )
            .execute(&mut *tx)
            .await?;
        }
    }

    let existing_ids: Vec<Uuid> = sqlx::query_scalar!(
        r#"
        SELECT ls.listing_id
        FROM listing_staff ls
        INNER JOIN listing l ON l.id = ls.listing_id
        WHERE ls.staff_user_id = $1
          AND ($2 OR l.user_id = $3)
        "#,
        staff_user_id,
        caller_is_admin,
        caller_id,
    )
    .fetch_all(&mut *tx)
    .await?;
    let existing_set: BTreeSet<Uuid> = existing_ids.into_iter().collect();

    if caller_is_admin {
        sqlx::query!(
            "DELETE FROM listing_staff WHERE staff_user_id = $1",
            staff_user_id,
        )
        .execute(&mut *tx)
        .await?;
    } else {
        sqlx::query!(
            r#"
            DELETE FROM listing_staff ls
            USING listing l
            WHERE ls.listing_id = l.id
              AND ls.staff_user_id = $1
              AND l.user_id = $2
            "#,
            staff_user_id,
            caller_id,
        )
        .execute(&mut *tx)
        .await?;
    }

    let mut added_names = Vec::new();
    for listing in &locked_listings {
        sqlx::query!(
            r#"
            INSERT INTO listing_staff (listing_id, staff_user_id, assigned_by)
            VALUES ($1, $2, $3)
            ON CONFLICT (listing_id, staff_user_id) DO NOTHING
            "#,
            listing.id,
            staff_user_id,
            caller_id,
        )
        .execute(&mut *tx)
        .await?;
        if !existing_set.contains(&listing.id) {
            added_names.push(listing.name.clone());
        }
    }
    added_names.sort();

    let outbox_email_id = if !added_names.is_empty() && !is_password_pending(&target.password_hash)
    {
        let host_name = load_caller_display_name(&mut tx, caller_id).await?;
        let base_url = admin_base_url.trim_end_matches('/');
        let payload = serde_json::to_value(StaffListingAssignedEmailPayload {
            first_name: target.first_name,
            host_name,
            listing_names: added_names,
            dispatch_url: format!("{base_url}/admin/turnovers"),
        })
        .map_err(|e| DbError::ValidationError(format!("invalid assignment email payload: {e}")))?;
        let outbox = insert_email_outbox_conn(
            &mut tx,
            &target.email,
            "New turnover listings assigned on Our Places",
            EmailTemplate::StaffListingAssigned.as_str(),
            &payload,
            3,
        )
        .await?;
        Some(outbox.id)
    } else {
        None
    };

    let summary =
        load_scoped_staff_member(&mut tx, caller_id, caller_is_admin, staff_user_id).await?;
    tx.commit().await?;

    Ok((summary, outbox_email_id))
}

/// Invalidates prior unused `staff_invite` tokens and queues a fresh `StaffInvite` email
/// when the staff member's initial password setup is still pending.
#[tracing::instrument(skip(pool))]
pub async fn resend_staff_invite(
    pool: &PgPool,
    caller_id: Uuid,
    caller_is_admin: bool,
    staff_user_id: Uuid,
    admin_base_url: &str,
) -> Result<Uuid> {
    let mut tx = pool.begin().await?;
    set_lock_timeout(&mut tx).await?;

    let target = sqlx::query!(
        r#"
        SELECT id, email, first_name, password_hash, is_active, deleted_at, roles AS "roles: Vec<UserRole>"
        FROM "user"
        WHERE id = $1
          AND deleted_at IS NULL
        FOR UPDATE
        "#,
        staff_user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "lock staff user for invite resend"))?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    if !target.is_active {
        return Err(DbError::ValidationError(
            "Cannot resend an invite to an inactive user account.".to_string(),
        ));
    }

    if !caller_is_admin {
        let in_roster = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM host_staff WHERE host_user_id = $1 AND staff_user_id = $2
            ) AS "exists!"
            "#,
            caller_id,
            staff_user_id,
        )
        .fetch_one(&mut *tx)
        .await?;
        if !in_roster {
            return Err(DbError::Sqlx(sqlx::Error::RowNotFound));
        }
    } else if !target.roles.contains(&UserRole::Staff) {
        let has_host_link = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM host_staff WHERE staff_user_id = $1) AS "exists!""#,
            staff_user_id,
        )
        .fetch_one(&mut *tx)
        .await?;
        if !has_host_link {
            return Err(DbError::Sqlx(sqlx::Error::RowNotFound));
        }
    }

    if !is_password_pending(&target.password_hash) {
        return Err(DbError::ValidationError(
            "This staff member has already set their password and can use Forgot password to reset it."
                .to_string(),
        ));
    }

    let mut listing_names: Vec<String> = sqlx::query_scalar!(
        r#"
        SELECT l.name
        FROM listing_staff ls
        INNER JOIN listing l ON l.id = ls.listing_id
        WHERE ls.staff_user_id = $1
          AND l.deleted_at IS NULL
          AND ($2 OR l.user_id = $3)
        ORDER BY l.name ASC
        "#,
        staff_user_id,
        caller_is_admin,
        caller_id,
    )
    .fetch_all(&mut *tx)
    .await?;
    listing_names.dedup();

    let host_name = load_caller_display_name(&mut tx, caller_id).await?;
    let expires_at = Utc::now() + ActionTokenPurpose::StaffInvite.default_ttl();
    let raw_token = issue_action_token_conn(
        &mut tx,
        staff_user_id,
        ActionTokenPurpose::StaffInvite,
        expires_at,
    )
    .await?;
    let base_url = admin_base_url.trim_end_matches('/');
    let setup_url = format!("{base_url}/reset-password?token={raw_token}");
    let payload = serde_json::to_value(StaffInviteEmailPayload {
        first_name: target.first_name,
        host_name,
        listing_names,
        setup_url,
        expires_hours: 72,
    })
    .map_err(|e| DbError::ValidationError(format!("invalid invite email payload: {e}")))?;

    let outbox = insert_email_outbox_conn(
        &mut tx,
        &target.email,
        "Your Our Places Turnover Dispatch invite link",
        EmailTemplate::StaffInvite.as_str(),
        &payload,
        3,
    )
    .await?;

    tx.commit().await?;
    Ok(outbox.id)
}

/// Removes a staff member from the caller's roster (`host_staff`) and owned `listing_staff` rows.
/// When no `host_staff` or `listing_staff` rows remain across any host, strips `'staff'` from
/// `"user".roles` (snapshotting `user_history`), closes open `staff_invite` tokens, and revokes
/// active `"admin"` portal sessions so dispatch access ends immediately.
#[tracing::instrument(skip(pool))]
pub async fn remove_staff_member(
    pool: &PgPool,
    caller_id: Uuid,
    caller_is_admin: bool,
    staff_user_id: Uuid,
) -> Result<()> {
    let raw_email: Option<String> =
        sqlx::query_scalar!(r#"SELECT email FROM "user" WHERE id = $1"#, staff_user_id,)
            .fetch_optional(pool)
            .await?;
    let normalized_email = raw_email
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    let mut tx = pool.begin().await?;
    set_lock_timeout(&mut tx).await?;
    // Acquire advisory lock before locking `"user"` to match `invite_or_attach_staff` lock order.
    lock_email_advisory(&mut tx, &normalized_email).await?;

    let target = sqlx::query!(
        r#"
        SELECT id, roles AS "roles: Vec<UserRole>"
        FROM "user"
        WHERE id = $1
        FOR UPDATE
        "#,
        staff_user_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "lock user for staff removal"))?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    let (removed_listings, removed_hosts) = if caller_is_admin {
        let listings = sqlx::query!(
            "DELETE FROM listing_staff WHERE staff_user_id = $1",
            staff_user_id,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let hosts = sqlx::query!(
            "DELETE FROM host_staff WHERE staff_user_id = $1",
            staff_user_id,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        (listings, hosts)
    } else {
        let listings = sqlx::query!(
            r#"
            DELETE FROM listing_staff ls
            USING listing l
            WHERE ls.listing_id = l.id
              AND ls.staff_user_id = $1
              AND l.user_id = $2
            "#,
            staff_user_id,
            caller_id,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let hosts = sqlx::query!(
            "DELETE FROM host_staff WHERE host_user_id = $1 AND staff_user_id = $2",
            caller_id,
            staff_user_id,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        (listings, hosts)
    };

    if removed_listings == 0
        && removed_hosts == 0
        && (!caller_is_admin || !target.roles.contains(&UserRole::Staff))
    {
        return Err(DbError::Sqlx(sqlx::Error::RowNotFound));
    }

    let still_linked = sqlx::query_scalar!(
        r#"
        SELECT (
            EXISTS(SELECT 1 FROM host_staff WHERE staff_user_id = $1)
            OR EXISTS(SELECT 1 FROM listing_staff WHERE staff_user_id = $1)
        ) AS "linked!"
        "#,
        staff_user_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    let mut revoke_sessions = false;
    if !still_linked {
        if target.roles.contains(&UserRole::Staff) {
            lock_and_snapshot(&mut tx, staff_user_id).await?;
            sqlx::query!(
                r#"
                UPDATE "user"
                SET roles = array_remove(roles, 'staff'::user_role),
                    updated_at = NOW()
                WHERE id = $1
                "#,
                staff_user_id,
            )
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query!(
            r#"
            UPDATE account_action_token
            SET used_at = NOW()
            WHERE user_id = $1
              AND purpose = 'staff_invite'
              AND used_at IS NULL
            "#,
            staff_user_id,
        )
        .execute(&mut *tx)
        .await?;
        revoke_sessions = true;
    }

    tx.commit().await?;

    if revoke_sessions {
        SessionsDb::new(pool.clone())
            .delete_user_sessions(staff_user_id, Some("admin"))
            .await
            .map_err(|e| {
                tracing::error!(
                    %staff_user_id,
                    error = ?e,
                    "failed to revoke admin sessions after unlinking last staff host"
                );
                let sqlx_err = e
                    .downcast::<sqlx::Error>()
                    .unwrap_or_else(|other| sqlx::Error::Protocol(other.to_string()));
                DbError::Sqlx(sqlx_err)
            })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx_db_tester::TestPg;
    use std::path::Path;

    static DATABASE_URL_ENV: &str = "DATABASE_URL";

    fn test_db() -> TestPg {
        dotenvy::dotenv().ok();
        let db_url = match std::env::var(DATABASE_URL_ENV) {
            Ok(url) => url,
            Err(_) => "postgres://postgres:password@localhost:5432/our_places".to_string(),
        };
        TestPg::new(db_url, Path::new("./migrations"))
    }

    async fn seed_user(
        pool: &PgPool,
        email: &str,
        roles: &[&str],
    ) -> std::result::Result<Uuid, Box<dyn std::error::Error>> {
        let id = Uuid::now_v7();
        sqlx::query(
            r#"INSERT INTO "user" (id, email, password_hash, first_name, last_name, is_active, is_verified, attributes, roles, default_currency)
               VALUES ($1, $2, 'real-bcrypt-hash', 'Ada', 'Lovelace', TRUE, TRUE, '{}'::jsonb, $3::user_role[], 'USD')"#,
        )
        .bind(id)
        .bind(email)
        .bind(roles)
        .execute(pool)
        .await?;
        Ok(id)
    }

    async fn seed_listing(
        pool: &PgPool,
        owner_id: Uuid,
        name: &str,
    ) -> std::result::Result<Uuid, Box<dyn std::error::Error>> {
        let id = Uuid::now_v7();
        let slug = format!("s-{}", id.simple());
        sqlx::query(
            r#"INSERT INTO listing (id, user_id, name, listing_structure_id, country, slug, base_currency, added_at)
               VALUES ($1, $2, $3, 1, 'Jamaica', $4, 'USD', NOW())"#,
        )
        .bind(id)
        .bind(owner_id)
        .bind(name)
        .bind(slug)
        .execute(pool)
        .await?;
        Ok(id)
    }

    #[test]
    fn escape_like_pattern_escapes_wildcards_and_backslashes() {
        assert_eq!(escape_like_pattern(r"100%_a\b"), r"100\%\_a\\b");
        assert_eq!(escape_like_pattern("dana"), "dana");
    }

    #[tokio::test]
    async fn multi_role_user_keeps_existing_roles_on_attach_and_unlink()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let db = test_db();
        let pool = db.get_pool().await;
        let host = seed_user(&pool, "host@example.com", &["host"]).await?;
        let booker = seed_user(&pool, "guest.cleaner@example.com", &["booker"]).await?;
        let villa = seed_listing(&pool, host, "Palm Cove").await?;

        let out = invite_or_attach_staff(
            &pool,
            host,
            false,
            &InviteStaffRequest {
                email: "GUEST.CLEANER@EXAMPLE.COM".into(),
                first_name: Some("Ignored".into()),
                last_name: Some("Ignored".into()),
                listing_ids: vec![villa],
            },
            "https://admin.example.com",
        )
        .await?;
        assert_eq!(out.staff.user_id, booker);
        assert_eq!(out.staff.first_name, "Ada");
        assert!(!out.staff.password_pending);

        let roles_after_attach: Vec<String> =
            sqlx::query_scalar(r#"SELECT roles::text[] FROM "user" WHERE id = $1"#)
                .bind(booker)
                .fetch_one(&pool)
                .await?;
        assert!(roles_after_attach.contains(&"booker".to_string()));
        assert!(roles_after_attach.contains(&"staff".to_string()));

        remove_staff_member(&pool, host, false, booker).await?;
        let roles_after_remove: Vec<String> =
            sqlx::query_scalar(r#"SELECT roles::text[] FROM "user" WHERE id = $1"#)
                .bind(booker)
                .fetch_one(&pool)
                .await?;
        assert!(roles_after_remove.contains(&"booker".to_string()));
        assert!(!roles_after_remove.contains(&"staff".to_string()));
        Ok(())
    }

    #[tokio::test]
    async fn resend_invite_invalidates_prior_token_and_queues_fresh_email()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let db = test_db();
        let pool = db.get_pool().await;
        let host = seed_user(&pool, "host2@example.com", &["host"]).await?;
        let villa = seed_listing(&pool, host, "Coral Cliff").await?;

        let invited = invite_or_attach_staff(
            &pool,
            host,
            false,
            &InviteStaffRequest {
                email: "fresh.crew@example.com".into(),
                first_name: Some("Leo".into()),
                last_name: Some("Stone".into()),
                listing_ids: vec![villa],
            },
            "https://admin.example.com",
        )
        .await?;
        let staff_id = invited.staff.user_id;

        resend_staff_invite(&pool, host, false, staff_id, "https://admin.example.com").await?;

        let open_tokens: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM account_action_token WHERE user_id = $1 AND purpose = 'staff_invite' AND used_at IS NULL",
        )
        .bind(staff_id)
        .fetch_one(&pool)
        .await?;
        assert_eq!(open_tokens, 1);

        let outbox_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM email_outbox WHERE recipient_email = 'fresh.crew@example.com' AND template_id = 'staff_invite'",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(outbox_count, 2);
        Ok(())
    }
}
