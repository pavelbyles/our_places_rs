use anyhow::Result;
use sqlx::PgPool;

#[derive(Clone)]
pub struct SessionsDb {
    pool: PgPool,
}

impl SessionsDb {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Load session state by ID.
    /// Returns `Ok(Some(state))` if found and not expired.
    /// Returns `Ok(None)` if not found or expired (and deletes it if expired).
    pub async fn load(&self, id: &str) -> Result<Option<Vec<u8>>> {
        let now = chrono::Utc::now().timestamp();

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

    /// Save session state.
    /// `ttl_seconds` is the duration from now until expiration.
    pub async fn save(&self, id: &str, state: &[u8], ttl_seconds: i64) -> Result<()> {
        let expiration = chrono::Utc::now().timestamp() + ttl_seconds;

        sqlx::query!(
            r#"
            INSERT INTO sessions (id, state, ttl)
            VALUES ($1, $2, $3)
            ON CONFLICT (id) DO UPDATE
            SET state = EXCLUDED.state,
                ttl = EXCLUDED.ttl
            "#,
            id,
            state,
            expiration
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
}
