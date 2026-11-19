//! HTTP handlers for turnover dispatch board & cleaning status transitions.

use actix_web::{HttpResponse, web};
use api_core::auth::Claims;
use api_core::error::ApiError;
use common::email::EmailTemplate;
use common::turnover::{
    CreateManualTurnoverRequest, CreateTurnoverCommentRequest, TransitionTurnoverRequest,
    TurnoverActorRole, TurnoverCommentResponse, TurnoverDetailResponse, TurnoverListQuery,
    TurnoverListResponse, TurnoverResponse,
};
use db_core::models::UserRole;
use db_core::turnover::TurnoverActor;
use sqlx::PgPool;
use tracing::Instrument;
use uuid::Uuid;
use validator::Validate;

async fn resolve_dispatch_actor(pool: &PgPool, claims: &Claims) -> Result<TurnoverActor, ApiError> {
    let user = super::caller(pool, claims).await?;
    let role = if user.roles.contains(&UserRole::Admin) {
        TurnoverActorRole::Admin
    } else if user.roles.contains(&UserRole::Host) {
        TurnoverActorRole::Host
    } else if user.roles.contains(&UserRole::Staff) {
        TurnoverActorRole::Staff
    } else {
        tracing::warn!(
            user_id = %claims.sub,
            "non-dispatch user attempted to access turnover endpoint"
        );
        return Err(ApiError::Forbidden(
            "Turnover dispatch access requires host, staff, or admin role".into(),
        ));
    };

    Ok(TurnoverActor {
        user_id: user.id,
        role,
    })
}

/// Lists scoped turnover dispatch cards for the authenticated host, staff member, or admin.
#[tracing::instrument(skip(pool, claims, query), fields(user_id = %claims.sub))]
#[utoipa::path(
    get,
    path = "/api/v1/turnovers",
    tag = "turnovers",
    params(TurnoverListQuery),
    responses(
        (status = 200, description = "Scoped list of turnovers", body = TurnoverListResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn list_turnovers(
    pool: web::Data<PgPool>,
    claims: Claims,
    query: web::Query<TurnoverListQuery>,
) -> Result<HttpResponse, ApiError> {
    let actor = resolve_dispatch_actor(pool.get_ref(), &claims).await?;
    let list = db_core::turnover::list_turnovers(pool.get_ref(), actor, &query.into_inner())
        .await
        .map_err(ApiError::Database)?;
    Ok(HttpResponse::Ok().json(list))
}

fn spawn_publish_outbox_emails(
    publisher: Option<web::Data<api_core::email_publisher::EmailPublisher>>,
    outbox_ids: Vec<Uuid>,
    template: &'static str,
) {
    if outbox_ids.is_empty() {
        return;
    }
    let email_publisher = publisher
        .map(|p| p.get_ref().clone())
        .unwrap_or_else(api_core::email_publisher::EmailPublisher::from_env);
    let span = tracing::Span::current();
    tokio::spawn(
        async move {
            for email_id in outbox_ids {
                if let Err(e) = email_publisher.publish_email_event(email_id).await {
                    tracing::error!(
                        %email_id,
                        template,
                        error = ?e,
                        "turnover notification email queued but publish failed"
                    );
                }
            }
        }
        .instrument(span),
    );
}

/// Creates a manual turnover card on a listing visible to the caller.
#[tracing::instrument(skip(pool, claims, body, publisher), fields(user_id = %claims.sub, listing_id = %body.listing_id))]
#[utoipa::path(
    post,
    path = "/api/v1/turnovers",
    tag = "turnovers",
    request_body = CreateManualTurnoverRequest,
    responses(
        (status = 201, description = "Manual turnover created", body = TurnoverResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Listing not found or inaccessible"),
        (status = 503, description = "Service temporarily unavailable"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn create_manual_turnover(
    pool: web::Data<PgPool>,
    claims: Claims,
    body: web::Json<CreateManualTurnoverRequest>,
    publisher: Option<web::Data<api_core::email_publisher::EmailPublisher>>,
) -> Result<HttpResponse, ApiError> {
    body.validate()?;
    let actor = resolve_dispatch_actor(pool.get_ref(), &claims).await?;
    let admin_url = super::admin_base_url();
    let outcome = db_core::turnover::create_manual_turnover_with_outbox(
        pool.get_ref(),
        actor,
        &body,
        &admin_url,
    )
    .await
    .map_err(ApiError::Database)?;

    spawn_publish_outbox_emails(
        publisher,
        outcome.outbox_ids,
        EmailTemplate::TurnoverCreated.as_str(),
    );

    Ok(HttpResponse::Created().json(outcome.turnover))
}

/// Fetches full turnover detail, status history, and chronological comments for an authorized caller.
#[tracing::instrument(skip(pool, claims), fields(user_id = %claims.sub, turnover_id = %path))]
#[utoipa::path(
    get,
    path = "/api/v1/turnovers/{id}",
    tag = "turnovers",
    params(
        ("id" = Uuid, Path, description = "Turnover ID")
    ),
    responses(
        (status = 200, description = "Turnover detail", body = TurnoverDetailResponse),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Turnover not found or inaccessible"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn get_turnover_detail(
    pool: web::Data<PgPool>,
    claims: Claims,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let actor = resolve_dispatch_actor(pool.get_ref(), &claims).await?;
    let detail = db_core::turnover::get_turnover_detail(pool.get_ref(), actor, path.into_inner())
        .await
        .map_err(ApiError::Database)?;
    Ok(HttpResponse::Ok().json(detail))
}

/// Transitions a turnover's operational status under role-based rules and optional optimistic concurrency.
#[tracing::instrument(skip(pool, claims, body), fields(user_id = %claims.sub, turnover_id = %path))]
#[utoipa::path(
    post,
    path = "/api/v1/turnovers/{id}/status",
    tag = "turnovers",
    params(
        ("id" = Uuid, Path, description = "Turnover ID")
    ),
    request_body = TransitionTurnoverRequest,
    responses(
        (status = 200, description = "Turnover transitioned", body = TurnoverResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Transition forbidden for role"),
        (status = 404, description = "Turnover not found or inaccessible"),
        (status = 409, description = "Concurrent status conflict"),
        (status = 503, description = "Service temporarily unavailable"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn transition_turnover_status(
    pool: web::Data<PgPool>,
    claims: Claims,
    path: web::Path<Uuid>,
    body: web::Json<TransitionTurnoverRequest>,
) -> Result<HttpResponse, ApiError> {
    body.validate()?;
    let actor = resolve_dispatch_actor(pool.get_ref(), &claims).await?;
    let updated = db_core::turnover::transition_turnover_status(
        pool.get_ref(),
        actor,
        path.into_inner(),
        &body,
        "api::post_status",
    )
    .await
    .map_err(ApiError::Database)?;
    Ok(HttpResponse::Ok().json(updated))
}

/// Posts a crew/host comment on a scoped turnover and asynchronously publishes transactional outbox emails.
#[tracing::instrument(skip(pool, claims, body, publisher), fields(user_id = %claims.sub, turnover_id = %path))]
#[utoipa::path(
    post,
    path = "/api/v1/turnovers/{id}/comments",
    tag = "turnovers",
    params(
        ("id" = Uuid, Path, description = "Turnover ID")
    ),
    request_body = CreateTurnoverCommentRequest,
    responses(
        (status = 201, description = "Turnover comment created", body = TurnoverCommentResponse),
        (status = 400, description = "Validation error"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Turnover not found or inaccessible"),
        (status = 503, description = "Service temporarily unavailable"),
    ),
    security(("bearer_auth" = []))
)]
pub async fn post_turnover_comment(
    pool: web::Data<PgPool>,
    claims: Claims,
    path: web::Path<Uuid>,
    body: web::Json<CreateTurnoverCommentRequest>,
    publisher: Option<web::Data<api_core::email_publisher::EmailPublisher>>,
) -> Result<HttpResponse, ApiError> {
    body.validate()?;
    let actor = resolve_dispatch_actor(pool.get_ref(), &claims).await?;
    let admin_url = super::admin_base_url();
    let outcome = db_core::turnover::add_comment_with_outbox(
        pool.get_ref(),
        actor,
        path.into_inner(),
        &body.body,
        &admin_url,
    )
    .await
    .map_err(ApiError::Database)?;

    spawn_publish_outbox_emails(
        publisher,
        outcome.outbox_ids,
        EmailTemplate::TurnoverCommentNotification.as_str(),
    );

    Ok(HttpResponse::Created().json(outcome.comment))
}
