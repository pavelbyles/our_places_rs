//! Server-side portal sessions: create, read, delete, revoke.

use super::*;

#[tracing::instrument(skip_all)]
#[utoipa::path(
    post,
    path = "/api/v1/sessions",
    tag = "sessions",
    request_body = CreateSessionRequest,
    responses(
        (status = 201, description = "Session created", body = SessionResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn create_session_handler(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    body: web::Json<CreateSessionRequest>,
) -> Result<impl Responder, ApiError> {
    let req_data = body.into_inner();
    let sessions_db = db_core::sessions::SessionsDb::new(pool.get_ref().clone());
    let now = Utc::now();
    let expires_at = now.timestamp() + req_data.ttl_seconds;

    let session_resp = SessionResponse {
        token_hash: req_data.token_hash.clone(),
        user_id: req_data.user_id,
        email: req_data.email.clone(),
        name: req_data.name.clone(),
        role: req_data.role.clone(),
        namespace: req_data.namespace.clone(),
        created_at: now,
        last_accessed_at: now,
        expires_at,
    };

    let state_bytes = serde_json::to_vec(&session_resp).map_err(|_| ApiError::Internal)?;

    sessions_db
        .save_session(
            &req_data.token_hash,
            Some(req_data.user_id),
            Some(&req_data.email),
            &req_data.namespace,
            &state_bytes,
            req_data.ttl_seconds,
        )
        .await
        .map_err(|_| ApiError::Internal)?;

    Ok(respond(
        &req,
        Payload::Item(session_resp),
        |_: Vec<SessionResponse>| (),
        actix_web::http::StatusCode::CREATED,
    ))
}

#[tracing::instrument(skip_all)]
#[utoipa::path(
    get,
    path = "/api/v1/sessions/{token_hash}",
    tag = "sessions",
    params(
        ("token_hash" = String, Path, description = "Session token SHA-256 hash"),
        SessionNamespaceQuery
    ),
    responses(
        (status = 200, description = "Session found", body = SessionResponse),
        (status = 404, description = "Session not found or expired"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn get_session_handler(
    req: HttpRequest,
    pool: web::Data<PgPool>,
    path: web::Path<String>,
    query: web::Query<SessionNamespaceQuery>,
) -> Result<actix_web::HttpResponse, ApiError> {
    let token_hash = path.into_inner();
    let sessions_db = db_core::sessions::SessionsDb::new(pool.get_ref().clone());
    let ns = query.namespace.as_deref();

    // Touch and extend session by 30 days (2,592,000s) on active access
    let record = sessions_db
        .load_and_touch(&token_hash, ns, Some(2592000))
        .await
        .map_err(|_| ApiError::Internal)?;

    match record {
        Some(r) => {
            let session: SessionResponse =
                serde_json::from_slice(&r.state).unwrap_or_else(|_| SessionResponse {
                    token_hash: r.id,
                    user_id: r.user_id.unwrap_or_default(),
                    email: r.email.unwrap_or_default(),
                    name: String::new(),
                    role: "guest".to_string(),
                    namespace: r.namespace,
                    created_at: r.created_at,
                    last_accessed_at: r.last_accessed_at,
                    expires_at: r.ttl,
                });

            Ok(respond(
                &req,
                Payload::Item(session),
                |_: Vec<SessionResponse>| (),
                actix_web::http::StatusCode::OK,
            ))
        }
        None => Ok(actix_web::HttpResponse::NotFound().json(serde_json::json!({
            "code": "SESSION_NOT_FOUND",
            "message": "Session not found or expired"
        }))),
    }
}

#[tracing::instrument]
#[utoipa::path(
    delete,
    path = "/api/v1/sessions/{token_hash}",
    tag = "sessions",
    params(
        ("token_hash" = String, Path, description = "Session token SHA-256 hash")
    ),
    responses(
        (status = 204, description = "Session deleted"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn delete_session_handler(
    pool: web::Data<PgPool>,
    path: web::Path<String>,
) -> Result<actix_web::HttpResponse, ApiError> {
    let token_hash = path.into_inner();
    let sessions_db = db_core::sessions::SessionsDb::new(pool.get_ref().clone());
    sessions_db
        .delete(&token_hash)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(actix_web::HttpResponse::NoContent().finish())
}

#[tracing::instrument]
#[utoipa::path(
    delete,
    path = "/api/v1/users/{user_id}/sessions",
    tag = "sessions",
    params(
        ("user_id" = Uuid, Path, description = "User UUID"),
        SessionNamespaceQuery
    ),
    responses(
        (status = 204, description = "User sessions revoked"),
        (status = 500, description = "Internal server error")
    )
)]
pub(super) async fn revoke_user_sessions_handler(
    pool: web::Data<PgPool>,
    path: web::Path<Uuid>,
    query: web::Query<SessionNamespaceQuery>,
) -> Result<actix_web::HttpResponse, ApiError> {
    let user_id = path.into_inner();
    let sessions_db = db_core::sessions::SessionsDb::new(pool.get_ref().clone());
    let ns = query.namespace.as_deref();
    sessions_db
        .delete_user_sessions(user_id, ns)
        .await
        .map_err(|_| ApiError::Internal)?;
    Ok(actix_web::HttpResponse::NoContent().finish())
}
