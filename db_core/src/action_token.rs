//! Single-use SHA-256 hashed tokens for staff onboarding invites and password resets.

use crate::error::{DbError, Result};
use chrono::{DateTime, Duration, Utc};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use std::fmt;
use uuid::Uuid;

/// Prefix marker stored in `"user".password_hash` when an invited user has not
/// yet chosen a password. Never verifies under bcrypt.
pub const LOCKED_PASSWORD_SENTINEL_PREFIX: &str = "!locked!";

/// Intent of an [`ActionTokenPurpose`] row in `account_action_token`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionTokenPurpose {
    /// Initial password-setup invitation emailed to newly created crew accounts.
    StaffInvite,
    /// Self-service password reset emailed via `/forgot-password`.
    PasswordReset,
}

impl ActionTokenPurpose {
    /// Database identifier persisted in `account_action_token.purpose`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StaffInvite => "staff_invite",
            Self::PasswordReset => "password_reset",
        }
    }

    /// Standard validity duration for this token purpose.
    pub const fn default_ttl(self) -> Duration {
        match self {
            Self::StaffInvite => Duration::hours(72),
            Self::PasswordReset => Duration::minutes(30),
        }
    }
}

impl fmt::Display for ActionTokenPurpose {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Formats arbitrary bytes as lowercase hexadecimal.
fn to_lower_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

/// Generates a CSPRNG 32-byte URL-safe hex token for email action links.
pub fn generate_raw_token() -> String {
    let mut bytes = [0u8; 32];
    rand::fill(&mut bytes);
    to_lower_hex(&bytes)
}

/// Computes the lowercase hex SHA-256 digest stored in `account_action_token.token_hash`.
pub fn hash_token(raw_token: &str) -> String {
    let digest = Sha256::digest(raw_token.as_bytes());
    to_lower_hex(&digest)
}

/// Generates an unguessable non-bcrypt sentinel password hash for invited staff accounts.
pub fn locked_password_sentinel() -> String {
    format!("{LOCKED_PASSWORD_SENTINEL_PREFIX}{}", generate_raw_token())
}

/// Returns true when the stored password hash indicates an invited account still
/// awaiting its initial password setup.
pub fn is_password_pending(password_hash: &str) -> bool {
    password_hash.starts_with(LOCKED_PASSWORD_SENTINEL_PREFIX)
}

/// Invalidates any open tokens for `(user_id, purpose)` and inserts a fresh
/// hashed token on the caller's open connection, returning the plaintext token
/// for email delivery.
#[tracing::instrument(skip(conn))]
pub async fn issue_action_token_conn(
    conn: &mut PgConnection,
    user_id: Uuid,
    purpose: ActionTokenPurpose,
    expires_at: DateTime<Utc>,
) -> Result<String> {
    let purpose_str = purpose.as_str();

    sqlx::query!(
        r#"
        UPDATE account_action_token
        SET used_at = NOW()
        WHERE user_id = $1
          AND purpose = $2
          AND used_at IS NULL
        "#,
        user_id,
        purpose_str,
    )
    .execute(&mut *conn)
    .await?;

    let raw_token = generate_raw_token();
    let token_hash = hash_token(&raw_token);

    sqlx::query!(
        r#"
        INSERT INTO account_action_token (user_id, purpose, token_hash, expires_at)
        VALUES ($1, $2, $3, $4)
        "#,
        user_id,
        purpose_str,
        token_hash,
        expires_at,
    )
    .execute(&mut *conn)
    .await?;

    Ok(raw_token)
}

/// Standalone wrapper around [`issue_action_token_conn`] inside its own transaction.
#[tracing::instrument(skip(pool))]
pub async fn issue_action_token(
    pool: &PgPool,
    user_id: Uuid,
    purpose: ActionTokenPurpose,
    expires_at: DateTime<Utc>,
) -> Result<String> {
    let mut tx = pool.begin().await?;
    let raw = issue_action_token_conn(&mut tx, user_id, purpose, expires_at).await?;
    tx.commit().await?;
    Ok(raw)
}

/// Atomically redeems a single-use action token and updates the owner's password.
///
/// Claims the token with `UPDATE ... WHERE used_at IS NULL AND expires_at > clock_timestamp()`,
/// locks the owning `"user"` row `FOR UPDATE`, verifies `is_active = TRUE` and
/// `deleted_at IS NULL`, snapshots history into `user_history`, updates `password_hash`,
/// and sets `is_verified = TRUE`. Returns `Ok(None)` when missing, expired, already used,
/// or belonging to an inactive/deleted account.
#[tracing::instrument(skip(pool, raw_token, new_password_hash))]
pub async fn consume_action_token_and_set_password(
    pool: &PgPool,
    raw_token: &str,
    new_password_hash: &str,
) -> Result<Option<Uuid>> {
    let trimmed = raw_token.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let token_hash = hash_token(trimmed);
    let mut tx = pool.begin().await?;
    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *tx)
        .await?;

    let claimed_user = sqlx::query_scalar!(
        r#"
        UPDATE account_action_token
        SET used_at = NOW()
        WHERE token_hash = $1
          AND used_at IS NULL
          AND expires_at > clock_timestamp()
        RETURNING user_id
        "#,
        token_hash,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| DbError::from_sqlx_contended(e, "redeem action token"))?;

    let Some(user_id) = claimed_user else {
        tx.rollback().await?;
        return Ok(None);
    };

    let current = crate::user::lock_and_snapshot(&mut tx, user_id).await?;
    if !current.is_active || current.deleted_at.is_some() {
        tx.rollback().await?;
        return Ok(None);
    }

    sqlx::query!(
        r#"
        UPDATE "user"
        SET
            password_hash = $2,
            is_verified = TRUE,
            verification_code = NULL,
            verification_code_expires_at = NULL,
            updated_at = NOW()
        WHERE id = $1
        "#,
        user_id,
        new_password_hash,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(Some(user_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::promo_tests::{migrated_db, raw_user};

    #[test]
    fn token_hashing_and_sentinel_helpers() {
        let raw = generate_raw_token();
        let hash = hash_token(&raw);
        assert_eq!(raw.len(), 64);
        assert_eq!(hash.len(), 64);
        assert_ne!(raw, hash);
        assert_eq!(hash, hash_token(&raw));

        let sentinel = locked_password_sentinel();
        assert!(is_password_pending(&sentinel));
        assert!(!is_password_pending("$2b$12$examplehash"));
    }

    #[tokio::test]
    async fn reissuing_token_invalidates_prior_token_and_expiry_is_rejected()
    -> std::result::Result<(), Box<dyn std::error::Error>> {
        let db = migrated_db();
        let pool = db.get_pool().await;
        let user_id = raw_user(&pool).await;

        let t1 = issue_action_token(
            &pool,
            user_id,
            ActionTokenPurpose::PasswordReset,
            Utc::now() + Duration::minutes(30),
        )
        .await?;
        let t2 = issue_action_token(
            &pool,
            user_id,
            ActionTokenPurpose::PasswordReset,
            Utc::now() + Duration::minutes(30),
        )
        .await?;

        // Prior token cannot be consumed.
        assert_eq!(
            consume_action_token_and_set_password(&pool, &t1, "$2b$04$hash1").await?,
            None
        );

        // Fresh token succeeds once, and replays return None.
        assert_eq!(
            consume_action_token_and_set_password(&pool, &t2, "$2b$04$hash2").await?,
            Some(user_id)
        );
        assert_eq!(
            consume_action_token_and_set_password(&pool, &t2, "$2b$04$hash3").await?,
            None
        );

        // Expired token is rejected.
        let expired = issue_action_token(
            &pool,
            user_id,
            ActionTokenPurpose::StaffInvite,
            Utc::now() - Duration::seconds(5),
        )
        .await?;
        assert_eq!(
            consume_action_token_and_set_password(&pool, &expired, "$2b$04$hash4").await?,
            None
        );

        // Deactivated user's valid token is rejected without mutating password.
        let inactive_token = issue_action_token(
            &pool,
            user_id,
            ActionTokenPurpose::PasswordReset,
            Utc::now() + Duration::minutes(10),
        )
        .await?;
        sqlx::query(r#"UPDATE "user" SET is_active = FALSE WHERE id = $1"#)
            .bind(user_id)
            .execute(&pool)
            .await?;
        assert_eq!(
            consume_action_token_and_set_password(&pool, &inactive_token, "$2b$04$hash5").await?,
            None
        );
        Ok(())
    }
}
