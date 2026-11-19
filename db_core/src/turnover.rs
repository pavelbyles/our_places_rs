//! Operational turnover dispatch board persistence, transitions, and idempotent reconcilers.

use std::collections::{BTreeMap, HashSet};
use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use common::email::{
    EmailTemplate, TurnoverCommentNotificationPayload, TurnoverCreatedNotificationPayload,
};
use common::reference::listing_today;
use common::turnover::{
    CreateManualTurnoverRequest, MAX_TURNOVER_REASON_LEN, MAX_TURNOVER_TITLE_LEN,
    TransitionTurnoverRequest, TurnoverActorRole, TurnoverCommentResponse, TurnoverDetailResponse,
    TurnoverListQuery, TurnoverListResponse, TurnoverResponse, TurnoverSourceKind, TurnoverStatus,
    TurnoverStatusHistoryEntry, can_transition,
};
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::error::{DbError, Result};
use crate::models::BookingStatus;

/// Authenticated dispatch actor carrying their user ID and portal dispatch role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnoverActor {
    /// Authenticated caller user identifier.
    pub user_id: Uuid,
    /// Portal role used for access scoping and transition authorization.
    pub role: TurnoverActorRole,
}

#[derive(Debug)]
struct TurnoverRow {
    id: Uuid,
    listing_id: Uuid,
    listing_name: String,
    listing_city: Option<String>,
    checkin_policy_text: Option<String>,
    checkout_policy_text: Option<String>,
    source_kind: String,
    checkout_date: NaiveDate,
    next_checkin_date: Option<NaiveDate>,
    status: String,
    title: Option<String>,
    comment_count: i64,
    updated_at: DateTime<Utc>,
}

async fn set_lock_timeout(conn: &mut PgConnection) -> Result<()> {
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *conn)
        .await
        .map_err(|e| DbError::from_sqlx_contended(e, "turnover set lock_timeout"))?;
    Ok(())
}

async fn lock_listing(conn: &mut PgConnection, listing_id: Uuid) -> Result<Option<(Uuid, bool)>> {
    let row = sqlx::query!(
        r#"
        SELECT id, user_id, (deleted_at IS NOT NULL) AS "is_deleted!"
        FROM listing
        WHERE id = $1
        FOR UPDATE
        "#,
        listing_id,
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "lock listing for turnover"))?;
    Ok(row.map(|r| (r.user_id, r.is_deleted)))
}

async fn actor_has_listing_access(
    conn: &mut PgConnection,
    actor: TurnoverActor,
    listing_id: Uuid,
    owner_user_id: Uuid,
) -> Result<bool> {
    match actor.role {
        TurnoverActorRole::Admin => Ok(true),
        TurnoverActorRole::Host => Ok(owner_user_id == actor.user_id),
        TurnoverActorRole::Staff => {
            let assigned = sqlx::query_scalar!(
                r#"
                SELECT EXISTS(
                    SELECT 1 FROM listing_staff
                    WHERE listing_id = $1 AND staff_user_id = $2
                ) AS "assigned!"
                "#,
                listing_id,
                actor.user_id,
            )
            .fetch_one(&mut *conn)
            .await?;
            Ok(assigned)
        }
    }
}

fn parse_status(raw: &str) -> Result<TurnoverStatus> {
    TurnoverStatus::from_str(raw).map_err(DbError::ValidationError)
}

fn parse_source_kind(raw: &str) -> Result<TurnoverSourceKind> {
    TurnoverSourceKind::from_str(raw).map_err(DbError::ValidationError)
}

fn clean_optional_text(raw: Option<&str>, max_chars: usize, label: &str) -> Result<Option<String>> {
    let Some(trimmed) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if trimmed.chars().count() > max_chars {
        return Err(DbError::ValidationError(format!(
            "{label} must be {max_chars} characters or fewer."
        )));
    }
    Ok(Some(trimmed.to_string()))
}

struct HistoryRecord<'a> {
    turnover_id: Uuid,
    from_status: Option<TurnoverStatus>,
    to_status: TurnoverStatus,
    actor_user_id: Option<Uuid>,
    actor_role: &'a str,
    entry_point: &'a str,
    reason: Option<&'a str>,
}

async fn append_history(conn: &mut PgConnection, rec: HistoryRecord<'_>) -> Result<()> {
    let from_str = rec.from_status.map(|s| s.as_str().to_string());
    let to_str = rec.to_status.as_str();
    sqlx::query!(
        r#"
        INSERT INTO turnover_status_history (
            turnover_id, from_status, to_status,
            actor_user_id, actor_role, entry_point, reason
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        "#,
        rec.turnover_id,
        from_str,
        to_str,
        rec.actor_user_id,
        rec.actor_role,
        rec.entry_point,
        rec.reason,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn load_assigned_staff_map(
    conn: &mut PgConnection,
    listing_ids: &[Uuid],
) -> Result<BTreeMap<Uuid, Vec<String>>> {
    if listing_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let rows = sqlx::query!(
        r#"
        SELECT
            ls.listing_id,
            u.first_name,
            u.last_name,
            u.email
        FROM listing_staff ls
        INNER JOIN "user" u ON u.id = ls.staff_user_id
        WHERE ls.listing_id = ANY($1)
          AND u.is_active = TRUE
          AND u.deleted_at IS NULL
          AND 'staff'::user_role = ANY(u.roles)
        ORDER BY u.first_name ASC, u.last_name ASC, u.id ASC
        "#,
        listing_ids,
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut grouped: BTreeMap<Uuid, Vec<String>> = BTreeMap::new();
    for row in rows {
        let display = format!("{} {}", row.first_name, row.last_name)
            .trim()
            .to_string();
        let name = if display.is_empty() {
            row.email
        } else {
            display
        };
        grouped.entry(row.listing_id).or_default().push(name);
    }
    Ok(grouped)
}

fn hydrate_turnover_rows(
    rows: Vec<TurnoverRow>,
    mut staff_map: BTreeMap<Uuid, Vec<String>>,
) -> Result<Vec<TurnoverResponse>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(TurnoverResponse {
            id: row.id,
            listing_id: row.listing_id,
            listing_name: row.listing_name,
            listing_city: row.listing_city,
            checkin_policy_text: row.checkin_policy_text,
            checkout_policy_text: row.checkout_policy_text,
            source_kind: parse_source_kind(&row.source_kind)?,
            checkout_date: row.checkout_date,
            next_checkin_date: row.next_checkin_date,
            status: parse_status(&row.status)?,
            title: row.title,
            assigned_staff_names: staff_map.get(&row.listing_id).cloned().unwrap_or_default(),
            comment_count: row.comment_count,
            updated_at: row.updated_at,
        });
    }
    // Keep map moved cleanly so callers drop without unused warning if small
    staff_map.clear();
    Ok(out)
}

async fn load_scoped_turnover_summary(
    conn: &mut PgConnection,
    actor: TurnoverActor,
    turnover_id: Uuid,
) -> Result<TurnoverResponse> {
    let actor_role = actor.role.as_str();
    let row = sqlx::query_as!(
        TurnoverRow,
        r#"
        SELECT
            t.id,
            t.listing_id,
            l.name AS listing_name,
            l.city AS listing_city,
            l.listing_details->'policies'->>'check_in' AS checkin_policy_text,
            l.listing_details->'policies'->>'check_out' AS checkout_policy_text,
            t.source_kind,
            t.checkout_date,
            (
                SELECT MIN(nx.start_date)
                FROM (
                    SELECT b.date_from AS start_date
                    FROM booking b
                    WHERE b.listing_id = t.listing_id
                      AND b.id IS DISTINCT FROM t.booking_id
                      AND b.date_from >= t.checkout_date
                      AND (
                          b.status IN ('confirmed', 'completed')
                          OR (b.status = 'pending' AND b.expires_at > NOW())
                      )
                    UNION ALL
                    SELECT bd.date_from AS start_date
                    FROM listing_blocked_dates bd
                    WHERE bd.listing_id = t.listing_id
                      AND bd.id IS DISTINCT FROM t.blocked_date_id
                      AND bd.date_from >= t.checkout_date
                ) nx
            ) AS next_checkin_date,
            t.status,
            t.title,
            (SELECT COUNT(*) FROM turnover_comment tc WHERE tc.turnover_id = t.id) AS "comment_count!",
            t.updated_at
        FROM turnover t
        INNER JOIN listing l ON l.id = t.listing_id
        WHERE t.id = $1
          AND l.deleted_at IS NULL
          AND (
              $2 = 'admin'
              OR ($2 = 'host' AND l.user_id = $3)
              OR ($2 = 'staff' AND EXISTS (
                  SELECT 1 FROM listing_staff ls
                  WHERE ls.listing_id = l.id AND ls.staff_user_id = $3
              ))
          )
        "#,
        turnover_id,
        actor_role,
        actor.user_id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    let staff_map = load_assigned_staff_map(conn, &[row.listing_id]).await?;
    let mut list = hydrate_turnover_rows(vec![row], staff_map)?;
    list.pop().ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))
}

/// Lists scoped turnover events filtered by window (`today | upcoming | past | all`), status,
/// and optional listing ID with dynamically projected `next_checkin_date` and zero guest PII.
#[tracing::instrument(skip(pool))]
pub async fn list_turnovers(
    pool: &PgPool,
    actor: TurnoverActor,
    query: &TurnoverListQuery,
) -> Result<TurnoverListResponse> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(20).clamp(1, 100);
    let offset = i64::from(page.saturating_sub(1)) * i64::from(per_page);
    let limit = i64::from(per_page);

    let today_jm = listing_today(Utc::now());
    let window_str = query.window.unwrap_or_default().as_str();
    let status_str = query.status.map(|s| s.as_str().to_string());
    let actor_role = actor.role.as_str();

    let mut conn = pool.acquire().await?;

    let total = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) AS "count!"
        FROM turnover t
        INNER JOIN listing l ON l.id = t.listing_id
        WHERE l.deleted_at IS NULL
          AND ($1::uuid IS NULL OR t.listing_id = $1)
          AND ($2::text IS NULL OR t.status = $2)
          AND (
              $3 = 'all'
              OR ($3 = 'today' AND t.checkout_date = $4)
              OR ($3 = 'upcoming' AND t.checkout_date > $4)
              OR ($3 = 'past' AND t.checkout_date < $4)
          )
          AND (
              $5 = 'admin'
              OR ($5 = 'host' AND l.user_id = $6)
              OR ($5 = 'staff' AND EXISTS (
                  SELECT 1 FROM listing_staff ls
                  WHERE ls.listing_id = l.id AND ls.staff_user_id = $6
              ))
          )
        "#,
        query.listing_id,
        status_str,
        window_str,
        today_jm,
        actor_role,
        actor.user_id,
    )
    .fetch_one(&mut *conn)
    .await?;

    let rows = sqlx::query_as!(
        TurnoverRow,
        r#"
        SELECT
            t.id,
            t.listing_id,
            l.name AS listing_name,
            l.city AS listing_city,
            l.listing_details->'policies'->>'check_in' AS checkin_policy_text,
            l.listing_details->'policies'->>'check_out' AS checkout_policy_text,
            t.source_kind,
            t.checkout_date,
            (
                SELECT MIN(nx.start_date)
                FROM (
                    SELECT b.date_from AS start_date
                    FROM booking b
                    WHERE b.listing_id = t.listing_id
                      AND b.id IS DISTINCT FROM t.booking_id
                      AND b.date_from >= t.checkout_date
                      AND (
                          b.status IN ('confirmed', 'completed')
                          OR (b.status = 'pending' AND b.expires_at > NOW())
                      )
                    UNION ALL
                    SELECT bd.date_from AS start_date
                    FROM listing_blocked_dates bd
                    WHERE bd.listing_id = t.listing_id
                      AND bd.id IS DISTINCT FROM t.blocked_date_id
                      AND bd.date_from >= t.checkout_date
                ) nx
            ) AS next_checkin_date,
            t.status,
            t.title,
            (SELECT COUNT(*) FROM turnover_comment tc WHERE tc.turnover_id = t.id) AS "comment_count!",
            t.updated_at
        FROM turnover t
        INNER JOIN listing l ON l.id = t.listing_id
        WHERE l.deleted_at IS NULL
          AND ($1::uuid IS NULL OR t.listing_id = $1)
          AND ($2::text IS NULL OR t.status = $2)
          AND (
              $3 = 'all'
              OR ($3 = 'today' AND t.checkout_date = $4)
              OR ($3 = 'upcoming' AND t.checkout_date > $4)
              OR ($3 = 'past' AND t.checkout_date < $4)
          )
          AND (
              $5 = 'admin'
              OR ($5 = 'host' AND l.user_id = $6)
              OR ($5 = 'staff' AND EXISTS (
                  SELECT 1 FROM listing_staff ls
                  WHERE ls.listing_id = l.id AND ls.staff_user_id = $6
              ))
          )
        ORDER BY
            CASE WHEN $3 = 'past' THEN t.checkout_date END DESC,
            CASE WHEN $3 <> 'past' THEN t.checkout_date END ASC,
            l.name ASC,
            t.id ASC
        LIMIT $7 OFFSET $8
        "#,
        query.listing_id,
        status_str,
        window_str,
        today_jm,
        actor_role,
        actor.user_id,
        limit,
        offset,
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut listing_ids: Vec<Uuid> = rows.iter().map(|r| r.listing_id).collect();
    listing_ids.sort_unstable();
    listing_ids.dedup();
    let staff_map = load_assigned_staff_map(&mut conn, &listing_ids).await?;
    let turnovers = hydrate_turnover_rows(rows, staff_map)?;

    Ok(TurnoverListResponse {
        turnovers,
        total,
        page,
        per_page,
    })
}

/// Fetches a single scoped turnover along with chronological status audit history and comments.
#[tracing::instrument(skip(pool))]
pub async fn get_turnover_detail(
    pool: &PgPool,
    actor: TurnoverActor,
    turnover_id: Uuid,
) -> Result<TurnoverDetailResponse> {
    let mut conn = pool.acquire().await?;
    let turnover = load_scoped_turnover_summary(&mut conn, actor, turnover_id).await?;

    let history_rows = sqlx::query!(
        r#"
        SELECT
            h.id,
            h.turnover_id,
            h.from_status,
            h.to_status,
            h.actor_user_id,
            NULLIF(
                BTRIM(COALESCE(u.first_name, '') || ' ' || COALESCE(u.last_name, '')),
                ''
            ) AS actor_name,
            h.actor_role,
            h.entry_point,
            h.reason,
            h.created_at
        FROM turnover_status_history h
        LEFT JOIN "user" u ON u.id = h.actor_user_id
        WHERE h.turnover_id = $1
        ORDER BY h.created_at ASC, h.id ASC
        "#,
        turnover_id,
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut status_history = Vec::with_capacity(history_rows.len());
    for row in history_rows {
        let from_status = row.from_status.as_deref().map(parse_status).transpose()?;
        let to_status = parse_status(&row.to_status)?;
        status_history.push(TurnoverStatusHistoryEntry {
            id: row.id,
            turnover_id: row.turnover_id,
            from_status,
            to_status,
            actor_user_id: row.actor_user_id,
            actor_name: row.actor_name,
            actor_role: row.actor_role,
            entry_point: row.entry_point,
            reason: row.reason,
            created_at: row.created_at,
        });
    }

    let comment_rows = sqlx::query!(
        r#"
        SELECT
            id,
            turnover_id,
            author_user_id,
            author_name,
            author_role,
            body,
            created_at
        FROM turnover_comment
        WHERE turnover_id = $1
        ORDER BY created_at ASC, id ASC
        "#,
        turnover_id,
    )
    .fetch_all(&mut *conn)
    .await?;

    let comments = comment_rows
        .into_iter()
        .map(|row| TurnoverCommentResponse {
            id: row.id,
            turnover_id: row.turnover_id,
            author_user_id: row.author_user_id,
            author_name: row.author_name,
            author_role: row.author_role,
            body: row.body,
            created_at: row.created_at,
        })
        .collect();

    Ok(TurnoverDetailResponse {
        turnover,
        status_history,
        comments,
    })
}

static ADMIN_BASE_URL_ENV: &str = "ADMIN_BASE_URL";
const TURNOVER_CREATED_MAX_RETRIES: i32 = 3;

fn default_admin_base_url() -> String {
    std::env::var(ADMIN_BASE_URL_ENV)
        .ok()
        .map(|s| s.trim().trim_end_matches('/').to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://localhost:3012".to_string())
}

/// Output of creating a manual turnover together with outbox IDs queued in the same transaction.
#[derive(Debug, Clone)]
pub struct CreateManualTurnoverOutcome {
    /// Persisted turnover summary scoped to the creating actor.
    pub turnover: TurnoverResponse,
    /// `email_outbox` IDs inserted atomically for post-commit Pub/Sub publishing.
    pub outbox_ids: Vec<Uuid>,
}

struct TurnoverCreatedNotice<'a> {
    listing_id: Uuid,
    turnover_id: Uuid,
    checkout_date: NaiveDate,
    source_kind: TurnoverSourceKind,
    title: Option<&'a str>,
    exclude_user_id: Option<Uuid>,
    admin_base_url: &'a str,
}

/// Queues [`EmailTemplate::TurnoverCreated`] outbox notifications for every active verified
/// maintenance/cleaning staff member assigned to `listing_id`, excluding `exclude_user_id`.
async fn enqueue_turnover_created_emails_conn(
    conn: &mut PgConnection,
    notice: TurnoverCreatedNotice<'_>,
) -> Result<Vec<Uuid>> {
    let Some(listing_name) = sqlx::query_scalar!(
        r#"SELECT name FROM listing WHERE id = $1 AND deleted_at IS NULL"#,
        notice.listing_id,
    )
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(Vec::new());
    };

    let recipients = sqlx::query!(
        r#"
        SELECT DISTINCT u.id, u.email
        FROM "user" u
        INNER JOIN listing_staff ls ON ls.staff_user_id = u.id
        WHERE ls.listing_id = $1
          AND 'staff'::user_role = ANY(u.roles)
          AND u.is_active = TRUE
          AND u.is_verified = TRUE
          AND u.deleted_at IS NULL
          AND ($2::uuid IS NULL OR u.id <> $2)
        ORDER BY u.id
        "#,
        notice.listing_id,
        notice.exclude_user_id,
    )
    .fetch_all(&mut *conn)
    .await?;

    if recipients.is_empty() {
        return Ok(Vec::new());
    }

    let base_url = notice.admin_base_url.trim_end_matches('/');
    let payload = TurnoverCreatedNotificationPayload {
        listing_name: listing_name.clone(),
        checkout_date: notice.checkout_date.format("%Y-%m-%d").to_string(),
        source_label: notice.source_kind.label().to_string(),
        title: notice.title.map(ToString::to_string),
        turnover_url: format!("{base_url}/admin/turnovers/{}", notice.turnover_id),
    };
    let payload_value = serde_json::to_value(&payload)
        .map_err(|e| DbError::ValidationError(format!("turnover created email payload: {e}")))?;
    let subject = format!("New turnover for {listing_name} ({})", notice.checkout_date);
    let template_id = EmailTemplate::TurnoverCreated.as_str();

    let mut outbox_ids = Vec::with_capacity(recipients.len());
    for recipient in recipients {
        let outbox = crate::email_outbox::insert_email_outbox_conn(
            &mut *conn,
            &recipient.email,
            &subject,
            template_id,
            &payload_value,
            TURNOVER_CREATED_MAX_RETRIES,
        )
        .await?;
        outbox_ids.push(outbox.id);
    }

    Ok(outbox_ids)
}

/// Creates a manual turnover or off-booking maintenance item for a listing in the caller's scope.
#[tracing::instrument(skip(pool, req))]
pub async fn create_manual_turnover(
    pool: &PgPool,
    actor: TurnoverActor,
    req: &CreateManualTurnoverRequest,
) -> Result<TurnoverResponse> {
    create_manual_turnover_with_outbox(pool, actor, req, &default_admin_base_url())
        .await
        .map(|outcome| outcome.turnover)
}

/// Creates a manual turnover and returns both the turnover summary and queued outbox IDs for
/// immediate post-commit Pub/Sub publishing.
#[tracing::instrument(skip(pool, req, admin_base_url))]
pub async fn create_manual_turnover_with_outbox(
    pool: &PgPool,
    actor: TurnoverActor,
    req: &CreateManualTurnoverRequest,
    admin_base_url: &str,
) -> Result<CreateManualTurnoverOutcome> {
    let initial_status = req.initial_status.unwrap_or(TurnoverStatus::NeedsCleaning);
    if initial_status != TurnoverStatus::NeedsCleaning {
        can_transition(
            actor.role,
            TurnoverSourceKind::Manual,
            TurnoverStatus::NeedsCleaning,
            initial_status,
        )?;
    }
    let clean_title = clean_optional_text(req.title.as_deref(), MAX_TURNOVER_TITLE_LEN, "Title")?;

    let mut tx = pool.begin().await?;
    set_lock_timeout(&mut tx).await?;

    let Some((owner_user_id, is_deleted)) = lock_listing(&mut tx, req.listing_id).await? else {
        return Err(DbError::Sqlx(sqlx::Error::RowNotFound));
    };
    if is_deleted {
        return Err(DbError::ValidationError(
            "Cannot create a turnover for a deleted listing.".to_string(),
        ));
    }
    if !actor_has_listing_access(&mut tx, actor, req.listing_id, owner_user_id).await? {
        return Err(DbError::Forbidden(
            "You do not have dispatch access to this listing.".to_string(),
        ));
    }

    let status_str = initial_status.as_str();
    let turnover_id = sqlx::query_scalar!(
        r#"
        INSERT INTO turnover (
            listing_id, source_kind, checkout_date, status, title, created_by
        )
        VALUES ($1, 'manual', $2, $3, $4, $5)
        RETURNING id
        "#,
        req.listing_id,
        req.checkout_date,
        status_str,
        clean_title.as_deref(),
        actor.user_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    append_history(
        &mut tx,
        HistoryRecord {
            turnover_id,
            from_status: None,
            to_status: initial_status,
            actor_user_id: Some(actor.user_id),
            actor_role: actor.role.as_str(),
            entry_point: "manual_create",
            reason: clean_title.as_deref(),
        },
    )
    .await?;

    let outbox_ids = enqueue_turnover_created_emails_conn(
        &mut tx,
        TurnoverCreatedNotice {
            listing_id: req.listing_id,
            turnover_id,
            checkout_date: req.checkout_date,
            source_kind: TurnoverSourceKind::Manual,
            title: clean_title.as_deref(),
            exclude_user_id: Some(actor.user_id),
            admin_base_url,
        },
    )
    .await?;

    let summary = load_scoped_turnover_summary(&mut tx, actor, turnover_id).await?;
    tx.commit().await?;
    Ok(CreateManualTurnoverOutcome {
        turnover: summary,
        outbox_ids,
    })
}

/// Transitions a turnover status under strict `Listing -> Turnover` locking, checking role
/// rules and optional `expected_status` optimistic concurrency. Idempotent if `from == to`.
#[tracing::instrument(skip(pool, req))]
pub async fn transition_turnover_status(
    pool: &PgPool,
    actor: TurnoverActor,
    turnover_id: Uuid,
    req: &TransitionTurnoverRequest,
    entry_point: &str,
) -> Result<TurnoverResponse> {
    let reason = clean_optional_text(req.reason.as_deref(), MAX_TURNOVER_REASON_LEN, "Reason")?;

    // Resolve parent listing ID without locking first so we always acquire `listing` BEFORE `turnover`.
    let listing_lookup = sqlx::query!(
        r#"
        SELECT t.listing_id
        FROM turnover t
        INNER JOIN listing l ON l.id = t.listing_id
        WHERE t.id = $1 AND l.deleted_at IS NULL
        "#,
        turnover_id,
    )
    .fetch_optional(pool)
    .await?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    let mut tx = pool.begin().await?;
    set_lock_timeout(&mut tx).await?;

    let Some((owner_user_id, is_deleted)) =
        lock_listing(&mut tx, listing_lookup.listing_id).await?
    else {
        return Err(DbError::Sqlx(sqlx::Error::RowNotFound));
    };
    if is_deleted
        || !actor_has_listing_access(&mut tx, actor, listing_lookup.listing_id, owner_user_id)
            .await?
    {
        return Err(DbError::Sqlx(sqlx::Error::RowNotFound));
    }

    let current = sqlx::query!(
        r#"
        SELECT id, source_kind, status
        FROM turnover
        WHERE id = $1 AND listing_id = $2
        FOR UPDATE
        "#,
        turnover_id,
        listing_lookup.listing_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "lock turnover for status transition"))?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    let source_kind = parse_source_kind(&current.source_kind)?;
    let current_status = parse_status(&current.status)?;

    if let Some(expected) = req.expected_status
        && expected != current_status
    {
        return Err(DbError::TurnoverStatusConflict {
            expected,
            actual: current_status,
        });
    }

    can_transition(actor.role, source_kind, current_status, req.to_status)?;

    if current_status != req.to_status {
        let to_str = req.to_status.as_str();
        sqlx::query!(
            r#"
            UPDATE turnover
            SET status = $1,
                updated_at = NOW()
            WHERE id = $2
            "#,
            to_str,
            turnover_id,
        )
        .execute(&mut *tx)
        .await?;

        append_history(
            &mut tx,
            HistoryRecord {
                turnover_id,
                from_status: Some(current_status),
                to_status: req.to_status,
                actor_user_id: Some(actor.user_id),
                actor_role: actor.role.as_str(),
                entry_point,
                reason: reason.as_deref(),
            },
        )
        .await?;
    }

    let summary = load_scoped_turnover_summary(&mut tx, actor, turnover_id).await?;
    tx.commit().await?;
    Ok(summary)
}

/// Idempotently creates, updates, or marks `not_required` the booking turnover inside caller's
/// open transaction (`listing -> booking -> turnover` lock order).
#[tracing::instrument(skip(conn))]
pub async fn reconcile_for_booking_conn(
    conn: &mut PgConnection,
    booking_id: Uuid,
    entry_point: &str,
) -> Result<()> {
    set_lock_timeout(conn).await?;

    let Some(listing_id) =
        sqlx::query_scalar!("SELECT listing_id FROM booking WHERE id = $1", booking_id)
            .fetch_optional(&mut *conn)
            .await?
    else {
        return Ok(());
    };

    let Some((_owner_id, is_deleted)) = lock_listing(conn, listing_id).await? else {
        return Ok(());
    };
    if is_deleted {
        return Ok(());
    }

    let Some(booking) = sqlx::query!(
        r#"
        SELECT id, status AS "status: BookingStatus", date_to
        FROM booking
        WHERE id = $1
        FOR UPDATE
        "#,
        booking_id,
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "lock booking for turnover reconcile"))?
    else {
        return Ok(());
    };

    let existing = sqlx::query!(
        r#"
        SELECT id, checkout_date, status
        FROM turnover
        WHERE source_kind = 'booking' AND booking_id = $1
        ORDER BY created_at ASC
        FOR UPDATE
        "#,
        booking.id,
    )
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "lock booking turnover"))?;

    match booking.status {
        BookingStatus::Confirmed | BookingStatus::Completed => match existing {
            None => {
                let inserted = sqlx::query_scalar!(
                    r#"
                    INSERT INTO turnover (
                        listing_id, source_kind, booking_id, checkout_date, status
                    )
                    VALUES ($1, 'booking', $2, $3, 'needs_cleaning')
                    ON CONFLICT (booking_id) WHERE source_kind = 'booking' AND booking_id IS NOT NULL
                    DO NOTHING
                    RETURNING id
                    "#,
                    listing_id,
                    booking.id,
                    booking.date_to,
                )
                .fetch_optional(&mut *conn)
                .await?;

                if let Some(tid) = inserted {
                    append_history(
                        conn,
                        HistoryRecord {
                            turnover_id: tid,
                            from_status: None,
                            to_status: TurnoverStatus::NeedsCleaning,
                            actor_user_id: None,
                            actor_role: "system",
                            entry_point,
                            reason: None,
                        },
                    )
                    .await?;
                    enqueue_turnover_created_emails_conn(
                        conn,
                        TurnoverCreatedNotice {
                            listing_id,
                            turnover_id: tid,
                            checkout_date: booking.date_to,
                            source_kind: TurnoverSourceKind::Booking,
                            title: None,
                            exclude_user_id: None,
                            admin_base_url: &default_admin_base_url(),
                        },
                    )
                    .await?;
                }
            }
            Some(row) => {
                let current_status = parse_status(&row.status)?;
                if current_status == TurnoverStatus::NotRequired {
                    sqlx::query!(
                        r#"
                        UPDATE turnover
                        SET status = 'needs_cleaning',
                            checkout_date = $1,
                            updated_at = NOW()
                        WHERE id = $2
                        "#,
                        booking.date_to,
                        row.id,
                    )
                    .execute(&mut *conn)
                    .await?;
                    append_history(
                        conn,
                        HistoryRecord {
                            turnover_id: row.id,
                            from_status: Some(TurnoverStatus::NotRequired),
                            to_status: TurnoverStatus::NeedsCleaning,
                            actor_user_id: None,
                            actor_role: "system",
                            entry_point,
                            reason: None,
                        },
                    )
                    .await?;
                } else if row.checkout_date != booking.date_to {
                    sqlx::query!(
                        r#"
                        UPDATE turnover
                        SET checkout_date = $1,
                            updated_at = NOW()
                        WHERE id = $2
                        "#,
                        booking.date_to,
                        row.id,
                    )
                    .execute(&mut *conn)
                    .await?;
                }
            }
        },
        BookingStatus::Cancelled => {
            if let Some(row) = existing {
                let current_status = parse_status(&row.status)?;
                if current_status == TurnoverStatus::NeedsCleaning {
                    sqlx::query!(
                        r#"
                        UPDATE turnover
                        SET status = 'not_required',
                            updated_at = NOW()
                        WHERE id = $1
                        "#,
                        row.id,
                    )
                    .execute(&mut *conn)
                    .await?;
                    append_history(
                        conn,
                        HistoryRecord {
                            turnover_id: row.id,
                            from_status: Some(TurnoverStatus::NeedsCleaning),
                            to_status: TurnoverStatus::NotRequired,
                            actor_user_id: None,
                            actor_role: "system",
                            entry_point,
                            reason: Some("Booking cancelled"),
                        },
                    )
                    .await?;
                }
            }
        }
        BookingStatus::Pending => {}
    }

    Ok(())
}

/// Standalone transaction wrapper around [`reconcile_for_booking_conn`].
#[tracing::instrument(skip(pool))]
pub async fn reconcile_for_booking(
    pool: &PgPool,
    booking_id: Uuid,
    entry_point: &str,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    reconcile_for_booking_conn(&mut tx, booking_id, entry_point).await?;
    tx.commit().await?;
    Ok(())
}

/// Reconciles turnovers for external `ical_feed` blocks on `listing_id` inside caller's open
/// transaction (ignoring `manual` blocks, shifting `checkout_date` in place on extensions, and
/// marking removed future events `not_required` only when `checkout_date > today_jm` and
/// `skip_removals == false`), returning any `email_outbox` IDs queued for newly created turnovers.
#[tracing::instrument(skip(conn))]
pub async fn reconcile_for_listing_conn(
    conn: &mut PgConnection,
    listing_id: Uuid,
    feed_id: Option<Uuid>,
    skip_removals: bool,
    entry_point: &str,
) -> Result<Vec<Uuid>> {
    set_lock_timeout(conn).await?;

    let Some((_owner_id, is_deleted)) = lock_listing(conn, listing_id).await? else {
        return Ok(Vec::new());
    };
    if is_deleted {
        return Ok(Vec::new());
    }

    let blocks = sqlx::query!(
        r#"
        SELECT id, feed_id, external_uid, date_to
        FROM listing_blocked_dates
        WHERE listing_id = $1
          AND source = 'ical_feed'::calendar_block_source
          AND ($2::uuid IS NULL OR feed_id = $2)
          AND feed_id IS NOT NULL
          AND external_uid IS NOT NULL
        ORDER BY id
        FOR UPDATE
        "#,
        listing_id,
        feed_id,
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| {
        DbError::from_sqlx_contended(e, "lock listing iCal blocks for turnover reconcile")
    })?;

    let existing_turnovers = sqlx::query!(
        r#"
        SELECT id, blocked_date_id, source_feed_id, source_external_uid, checkout_date, status
        FROM turnover
        WHERE listing_id = $1
          AND source_kind = 'ical_feed'
          AND ($2::uuid IS NULL OR source_feed_id = $2)
        ORDER BY id
        FOR UPDATE
        "#,
        listing_id,
        feed_id,
    )
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "lock listing iCal turnovers"))?;

    struct ExistingIcalTurnover {
        id: Uuid,
        blocked_date_id: Option<Uuid>,
        checkout_date: NaiveDate,
        status: TurnoverStatus,
    }

    let mut by_key: BTreeMap<(Uuid, String), ExistingIcalTurnover> = BTreeMap::new();
    let mut orphaned_to_prune = Vec::new();
    for row in existing_turnovers {
        let status = parse_status(&row.status)?;
        match (row.source_feed_id, row.source_external_uid) {
            (Some(fid), Some(uid)) => {
                by_key.insert(
                    (fid, uid),
                    ExistingIcalTurnover {
                        id: row.id,
                        blocked_date_id: row.blocked_date_id,
                        checkout_date: row.checkout_date,
                        status,
                    },
                );
            }
            _ => {
                orphaned_to_prune.push((row.id, row.checkout_date, status));
            }
        }
    }

    let admin_base_url = default_admin_base_url();
    let mut outbox_ids = Vec::new();
    let mut seen_keys: HashSet<(Uuid, String)> = HashSet::new();
    for blk in blocks {
        let (Some(blk_feed_id), Some(blk_uid)) = (blk.feed_id, blk.external_uid) else {
            continue;
        };
        let key = (blk_feed_id, blk_uid.clone());
        seen_keys.insert(key.clone());

        match by_key.get(&key) {
            None => {
                let inserted = sqlx::query_scalar!(
                    r#"
                    INSERT INTO turnover (
                        listing_id, source_kind, blocked_date_id,
                        source_feed_id, source_external_uid, checkout_date, status
                    )
                    VALUES ($1, 'ical_feed', $2, $3, $4, $5, 'needs_cleaning')
                    ON CONFLICT (listing_id, source_feed_id, source_external_uid)
                    WHERE source_kind = 'ical_feed' AND source_feed_id IS NOT NULL AND source_external_uid IS NOT NULL
                    DO NOTHING
                    RETURNING id
                    "#,
                    listing_id,
                    blk.id,
                    blk_feed_id,
                    blk_uid,
                    blk.date_to,
                )
                .fetch_optional(&mut *conn)
                .await?;

                if let Some(tid) = inserted {
                    append_history(
                        conn,
                        HistoryRecord {
                            turnover_id: tid,
                            from_status: None,
                            to_status: TurnoverStatus::NeedsCleaning,
                            actor_user_id: None,
                            actor_role: "system",
                            entry_point,
                            reason: None,
                        },
                    )
                    .await?;
                    let mut queued = enqueue_turnover_created_emails_conn(
                        conn,
                        TurnoverCreatedNotice {
                            listing_id,
                            turnover_id: tid,
                            checkout_date: blk.date_to,
                            source_kind: TurnoverSourceKind::IcalFeed,
                            title: None,
                            exclude_user_id: None,
                            admin_base_url: &admin_base_url,
                        },
                    )
                    .await?;
                    outbox_ids.append(&mut queued);
                }
            }
            Some(existing) => {
                if existing.status == TurnoverStatus::NotRequired {
                    sqlx::query!(
                        r#"
                        UPDATE turnover
                        SET status = 'needs_cleaning',
                            blocked_date_id = $1,
                            checkout_date = $2,
                            updated_at = NOW()
                        WHERE id = $3
                        "#,
                        blk.id,
                        blk.date_to,
                        existing.id,
                    )
                    .execute(&mut *conn)
                    .await?;
                    append_history(
                        conn,
                        HistoryRecord {
                            turnover_id: existing.id,
                            from_status: Some(TurnoverStatus::NotRequired),
                            to_status: TurnoverStatus::NeedsCleaning,
                            actor_user_id: None,
                            actor_role: "system",
                            entry_point,
                            reason: None,
                        },
                    )
                    .await?;
                } else if existing.checkout_date != blk.date_to
                    || existing.blocked_date_id != Some(blk.id)
                {
                    sqlx::query!(
                        r#"
                        UPDATE turnover
                        SET blocked_date_id = $1,
                            checkout_date = $2,
                            updated_at = NOW()
                        WHERE id = $3
                        "#,
                        blk.id,
                        blk.date_to,
                        existing.id,
                    )
                    .execute(&mut *conn)
                    .await?;
                }
            }
        }
    }

    if !skip_removals {
        let today_jm = listing_today(Utc::now());
        for (key, existing) in by_key {
            if !seen_keys.contains(&key)
                && existing.status == TurnoverStatus::NeedsCleaning
                && existing.checkout_date > today_jm
            {
                sqlx::query!(
                    r#"
                    UPDATE turnover
                    SET status = 'not_required',
                        blocked_date_id = NULL,
                        updated_at = NOW()
                    WHERE id = $1
                    "#,
                    existing.id,
                )
                .execute(&mut *conn)
                .await?;
                append_history(
                    conn,
                    HistoryRecord {
                        turnover_id: existing.id,
                        from_status: Some(TurnoverStatus::NeedsCleaning),
                        to_status: TurnoverStatus::NotRequired,
                        actor_user_id: None,
                        actor_role: "system",
                        entry_point,
                        reason: Some("External iCal event removed before checkout"),
                    },
                )
                .await?;
            }
        }
        for (tid, checkout_date, status) in orphaned_to_prune {
            if status == TurnoverStatus::NeedsCleaning && checkout_date > today_jm {
                sqlx::query!(
                    r#"
                    UPDATE turnover
                    SET status = 'not_required',
                        blocked_date_id = NULL,
                        updated_at = NOW()
                    WHERE id = $1
                    "#,
                    tid,
                )
                .execute(&mut *conn)
                .await?;
                append_history(
                    conn,
                    HistoryRecord {
                        turnover_id: tid,
                        from_status: Some(TurnoverStatus::NeedsCleaning),
                        to_status: TurnoverStatus::NotRequired,
                        actor_user_id: None,
                        actor_role: "system",
                        entry_point,
                        reason: Some("External calendar feed removed"),
                    },
                )
                .await?;
            }
        }
    }

    Ok(outbox_ids)
}

/// Standalone transaction wrapper around [`reconcile_for_listing_conn`].
#[tracing::instrument(skip(pool))]
pub async fn reconcile_for_listing(
    pool: &PgPool,
    listing_id: Uuid,
    feed_id: Option<Uuid>,
    skip_removals: bool,
    entry_point: &str,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let _ = reconcile_for_listing_conn(&mut tx, listing_id, feed_id, skip_removals, entry_point)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Returns active (non-deleted) listing IDs ordered deterministically for backfill sweeps.
pub async fn list_active_listing_ids(pool: &PgPool) -> Result<Vec<Uuid>> {
    let ids = sqlx::query_scalar!("SELECT id FROM listing WHERE deleted_at IS NULL ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok(ids)
}

/// Backfills any out-of-sync booking or `ical_feed` turnovers for one listing inside a locked
/// transaction (`listing -> booking/blocked_dates -> turnover` with `5s` `lock_timeout`).
///
/// Passes `skip_removals = true` to [`reconcile_for_listing_conn`] because iCal event removal
/// pruning requires `had_future_blocks` + `NO_EVENTS_WARNING` context owned by `reconcile_feed`.
#[tracing::instrument(skip(pool))]
pub async fn backfill_listing_turnovers(
    pool: &PgPool,
    listing_id: Uuid,
    entry_point: &str,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    set_lock_timeout(&mut tx).await?;

    let Some((_owner_id, is_deleted)) = lock_listing(&mut tx, listing_id).await? else {
        return Ok(());
    };
    if is_deleted {
        return Ok(());
    }

    let booking_ids = sqlx::query_scalar!(
        r#"
        SELECT DISTINCT b.id
        FROM booking b
        LEFT JOIN turnover t
          ON t.booking_id = b.id AND t.source_kind = 'booking'
        WHERE b.listing_id = $1
          AND (
              (
                  b.status IN ('confirmed'::booking_status, 'completed'::booking_status)
                  AND (
                      t.id IS NULL
                      OR t.checkout_date <> b.date_to
                      OR t.status = 'not_required'
                  )
              )
              OR (
                  b.status = 'cancelled'::booking_status
                  AND t.id IS NOT NULL
                  AND t.status = 'needs_cleaning'
              )
          )
        ORDER BY b.id
        "#,
        listing_id,
    )
    .fetch_all(&mut *tx)
    .await?;

    for booking_id in booking_ids {
        reconcile_for_booking_conn(&mut tx, booking_id, entry_point).await?;
    }

    let _ = reconcile_for_listing_conn(&mut tx, listing_id, None, true, entry_point).await?;
    tx.commit().await?;
    Ok(())
}

/// Output of posting a turnover comment together with outbox IDs queued in the same transaction.
#[derive(Debug, Clone)]
pub struct AddCommentOutcome {
    /// Persisted turnover comment with immutable `author_name` snapshot.
    pub comment: TurnoverCommentResponse,
    /// `email_outbox` IDs inserted atomically for post-commit Pub/Sub publishing.
    pub outbox_ids: Vec<Uuid>,
}

const MAX_TURNOVER_COMMENT_LEN: usize = 2000;
const TURNOVER_COMMENT_MAX_RETRIES: i32 = 3;

/// Inserts a crew/host/admin comment onto a scoped turnover and atomically queues notification
/// emails (`TurnoverCommentNotification`) for the listing owner and active verified assigned
/// staff, excluding the author (`listing -> turnover` lock order under `5s` `lock_timeout`).
#[tracing::instrument(skip(pool, body, admin_base_url), fields(turnover_id = %turnover_id, user_id = %actor.user_id))]
pub async fn add_comment_with_outbox(
    pool: &PgPool,
    actor: TurnoverActor,
    turnover_id: Uuid,
    body: &str,
    admin_base_url: &str,
) -> Result<AddCommentOutcome> {
    let trimmed = body.trim();
    if trimmed.is_empty() || trimmed.chars().count() > MAX_TURNOVER_COMMENT_LEN {
        return Err(DbError::ValidationError(format!(
            "Comment must be between 1 and {MAX_TURNOVER_COMMENT_LEN} characters."
        )));
    }

    let listing_lookup = sqlx::query!(
        r#"
        SELECT t.listing_id
        FROM turnover t
        INNER JOIN listing l ON l.id = t.listing_id
        WHERE t.id = $1 AND l.deleted_at IS NULL
        "#,
        turnover_id,
    )
    .fetch_optional(pool)
    .await?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    let mut tx = pool.begin().await?;
    set_lock_timeout(&mut tx).await?;

    let Some((owner_user_id, is_deleted)) =
        lock_listing(&mut tx, listing_lookup.listing_id).await?
    else {
        return Err(DbError::Sqlx(sqlx::Error::RowNotFound));
    };
    if is_deleted
        || !actor_has_listing_access(&mut tx, actor, listing_lookup.listing_id, owner_user_id)
            .await?
    {
        return Err(DbError::Sqlx(sqlx::Error::RowNotFound));
    }

    let turnover = sqlx::query!(
        r#"
        SELECT id, checkout_date
        FROM turnover
        WHERE id = $1 AND listing_id = $2
        FOR UPDATE
        "#,
        turnover_id,
        listing_lookup.listing_id,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "lock turnover for comment"))?
    .ok_or(DbError::Sqlx(sqlx::Error::RowNotFound))?;

    let listing_name = sqlx::query_scalar!(
        "SELECT name FROM listing WHERE id = $1",
        listing_lookup.listing_id
    )
    .fetch_one(&mut *tx)
    .await?;

    let author_row = sqlx::query!(
        r#"
        SELECT first_name, last_name
        FROM "user"
        WHERE id = $1 AND deleted_at IS NULL
        "#,
        actor.user_id,
    )
    .fetch_optional(&mut *tx)
    .await?;
    let author_name = author_row
        .map(|u| {
            format!("{} {}", u.first_name, u.last_name)
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Crew Member".to_string());

    let inserted = sqlx::query!(
        r#"
        INSERT INTO turnover_comment (
            turnover_id, author_user_id, author_name, author_role, body
        )
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, turnover_id, author_user_id, author_name, author_role, body, created_at
        "#,
        turnover_id,
        actor.user_id,
        author_name,
        actor.role.as_str(),
        trimmed,
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        "UPDATE turnover SET updated_at = NOW() WHERE id = $1",
        turnover_id
    )
    .execute(&mut *tx)
    .await?;

    let recipients = sqlx::query!(
        r#"
        SELECT DISTINCT u.id, u.email
        FROM "user" u
        WHERE u.is_active = TRUE
          AND u.is_verified = TRUE
          AND u.deleted_at IS NULL
          AND u.id <> $1
          AND (
              u.id = $2
              OR EXISTS (
                  SELECT 1
                  FROM listing_staff ls
                  WHERE ls.listing_id = $3
                    AND ls.staff_user_id = u.id
                    AND 'staff'::user_role = ANY(u.roles)
              )
          )
        ORDER BY u.id
        "#,
        actor.user_id,
        owner_user_id,
        listing_lookup.listing_id,
    )
    .fetch_all(&mut *tx)
    .await?;

    let payload = TurnoverCommentNotificationPayload {
        listing_name: listing_name.clone(),
        checkout_date: turnover.checkout_date.format("%Y-%m-%d").to_string(),
        author_name: inserted.author_name.clone(),
        author_role: inserted.author_role.clone(),
        comment_body: inserted.body.clone(),
        turnover_url: format!(
            "{}/admin/turnovers/{turnover_id}",
            admin_base_url.trim_end_matches('/')
        ),
    };
    let payload_value = serde_json::to_value(&payload)
        .map_err(|e| DbError::ValidationError(format!("turnover comment email payload: {e}")))?;
    let subject = format!(
        "Turnover note for {listing_name} ({})",
        turnover.checkout_date
    );
    let template_id = EmailTemplate::TurnoverCommentNotification.as_str();

    let mut outbox_ids = Vec::with_capacity(recipients.len());
    for recipient in recipients {
        let outbox = crate::email_outbox::insert_email_outbox_conn(
            &mut tx,
            &recipient.email,
            &subject,
            template_id,
            &payload_value,
            TURNOVER_COMMENT_MAX_RETRIES,
        )
        .await?;
        outbox_ids.push(outbox.id);
    }

    tx.commit().await?;

    Ok(AddCommentOutcome {
        comment: TurnoverCommentResponse {
            id: inserted.id,
            turnover_id: inserted.turnover_id,
            author_user_id: inserted.author_user_id,
            author_name: inserted.author_name,
            author_role: inserted.author_role,
            body: inserted.body,
            created_at: inserted.created_at,
        },
        outbox_ids,
    })
}
