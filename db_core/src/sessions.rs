use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: String,
    pub user_id: Option<Uuid>,
    pub email: Option<String>,
    pub namespace: String,
    pub state: Vec<u8>,
    pub ttl: i64,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct SessionsDb {
    pool: PgPool,
}

impl SessionsDb {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Load session raw state by ID.
    /// Returns `Ok(Some(state))` if found and not expired.
    /// Returns `Ok(None)` if not found or expired (and deletes it if expired).
    pub async fn load(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let now = Utc::now().timestamp();

        let record = sqlx::query!("SELECT state, ttl FROM sessions WHERE id = $1", id)
            .fetch_optional(&self.pool)
            .await?;

        if let Some(r) = record {
            if r.ttl < now {
                // Expired, delete it
                let _ = self.delete(id).await;
                return Ok(None);
            }
            return Ok(Some(r.state));
        }

        Ok(None)
    }

    /// Load full session record and optionally verify namespace and extend TTL.
    pub async fn load_and_touch(
        &self,
        id: &str,
        expected_namespace: Option<&str>,
        extend_ttl_seconds: Option<i64>,
    ) -> Result<Option<SessionRecord>> {
        let now_dt = Utc::now();
        let now = now_dt.timestamp();

        let record = sqlx::query!(
            r#"
            SELECT id, user_id, email, namespace, state, ttl, created_at, last_accessed_at
            FROM sessions
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(r) = record {
            if r.ttl < now {
                let _ = self.delete(id).await;
                return Ok(None);
            }

            if let Some(ns) = expected_namespace {
                if r.namespace != ns {
                    return Ok(None);
                }
            }

            let new_ttl = if let Some(extend_sec) = extend_ttl_seconds {
                now + extend_sec
            } else {
                r.ttl
            };

            // Touch last_accessed_at and update ttl
            let _ = sqlx::query!(
                r#"
                UPDATE sessions
                SET last_accessed_at = $1, ttl = $2
                WHERE id = $3
                "#,
                now_dt,
                new_ttl,
                id
            )
            .execute(&self.pool)
            .await;

            return Ok(Some(SessionRecord {
                id: r.id,
                user_id: r.user_id,
                email: r.email,
                namespace: r.namespace,
                state: r.state,
                ttl: new_ttl,
                created_at: r.created_at,
                last_accessed_at: now_dt,
            }));
        }

        Ok(None)
    }

    /// Save session with default namespace.
    pub async fn save(&self, id: &str, state: &[u8], ttl_seconds: i64) -> Result<()> {
        self.save_session(id, None, None, "guest", state, ttl_seconds)
            .await
    }

    /// Save session with explicit user_id, email, and namespace.
    pub async fn save_session(
        &self,
        id: &str,
        user_id: Option<Uuid>,
        email: Option<&str>,
        namespace: &str,
        state: &[u8],
        ttl_seconds: i64,
    ) -> Result<()> {
        let now_dt = Utc::now();
        let expiration = now_dt.timestamp() + ttl_seconds;

        sqlx::query!(
            r#"
            INSERT INTO sessions (id, user_id, email, namespace, state, ttl, created_at, last_accessed_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (id) DO UPDATE
            SET user_id = EXCLUDED.user_id,
                email = EXCLUDED.email,
                namespace = EXCLUDED.namespace,
                state = EXCLUDED.state,
                ttl = EXCLUDED.ttl,
                last_accessed_at = EXCLUDED.last_accessed_at
            "#,
            id,
            user_id,
            email,
            namespace,
            state,
            expiration,
            now_dt,
            now_dt
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query!("DELETE FROM sessions WHERE id = $1", id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Revoke all active sessions for a user, optionally scoped to a specific namespace.
    pub async fn delete_user_sessions(&self, user_id: Uuid, namespace: Option<&str>) -> Result<u64> {
        let rows_affected = if let Some(ns) = namespace {
            sqlx::query!(
                "DELETE FROM sessions WHERE user_id = $1 AND namespace = $2",
                user_id,
                ns
            )
            .execute(&self.pool)
            .await?
            .rows_affected()
        } else {
            sqlx::query!("DELETE FROM sessions WHERE user_id = $1", user_id)
                .execute(&self.pool)
                .await?
                .rows_affected()
        };

        Ok(rows_affected)
    }
}
