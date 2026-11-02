use crate::error::{DbError, Result};
use crate::models::{
    BookerProfile, HostProfile, NewBookerProfile, NewHostProfile, NewUser, UpdatedUser, User,
    UserRole,
};
use chrono::Utc;
use sqlx::{PgConnection, PgExecutor, PgPool};
use uuid::Uuid;

/// Compile-time checked `query_as!(User, ...)` with the `User` column list
/// written once. sqlx's macros only accept string literals (no `const` or
/// `concat!`), but they do accept `"lit" + "lit"`, so the list is spliced in
/// as a literal.
///
/// - `query_user!(select "WHERE id = $1", id)` → `SELECT <cols> FROM "user" WHERE id = $1`
/// - `query_user!(returning "UPDATE \"user\" SET ... WHERE id = $1", id)` → `... RETURNING <cols>`
macro_rules! query_user {
    (@build [$($pre:literal)+] [$($post:literal)*] $(, $arg:expr)*) => {
        sqlx::query_as!(
            User,
            $($pre +)+
            r#"id, email, password_hash, first_name, last_name, phone_number, is_active, is_verified, verification_code, verification_code_expires_at, created_at, updated_at, attributes, roles as "roles: Vec<UserRole>", default_currency, deleted_at, slug"#
            $(+ $post)*
            $(, $arg)*
        )
    };
    (select $tail:literal $(, $arg:expr)* $(,)?) => {
        query_user!(@build ["SELECT "] [r#" FROM "user" "# $tail] $(, $arg)*)
    };
    (returning $stmt:literal $(, $arg:expr)* $(,)?) => {
        query_user!(@build [$stmt " RETURNING "] [] $(, $arg)*)
    };
}

/// Creates a new user in the database.
#[tracing::instrument(skip(executor))]
pub async fn create_user<'e, E>(executor: E, new_user_request: &NewUser) -> Result<User>
where
    E: PgExecutor<'e>,
{
    let now = Utc::now();
    let user = query_user!(
        returning
        r#"INSERT INTO "user" (id, email, password_hash, first_name, last_name, phone_number, is_active, is_verified, verification_code, verification_code_expires_at, created_at, updated_at, attributes, roles, default_currency)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)"#,
        new_user_request.id,
        new_user_request.email,
        new_user_request.password_hash,
        new_user_request.first_name,
        new_user_request.last_name,
        new_user_request.phone_number,
        new_user_request.is_active,
        new_user_request.is_verified,
        new_user_request.verification_code,
        new_user_request.verification_code_expires_at,
        now,
        now,
        new_user_request.attributes,
        new_user_request.roles.clone().unwrap_or_default() as Vec<UserRole>,
        new_user_request.default_currency,
    )
    .fetch_one(executor)
    .await?;

    Ok(user)
}

/// Turns a clash on the unique slug into a message for the user.
fn slug_taken(err: sqlx::Error) -> DbError {
    match err.as_database_error() {
        Some(db) if db.constraint() == Some("user_slug_unique") => {
            DbError::ValidationError("That profile URL is already taken.".to_string())
        }
        _ => DbError::Sqlx(err),
    }
}

/// Copies the user's current row into `user_history`, valid from its last update.
async fn record_user_history(conn: &mut PgConnection, current: &User) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO user_history
        (user_id, email, password_hash, first_name, last_name, phone_number, is_active, is_verified, valid_from, attributes, roles, default_currency)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
        current.id,
        current.email,
        current.password_hash,
        current.first_name,
        current.last_name,
        current.phone_number,
        current.is_active,
        current.is_verified,
        current.updated_at,
        current.attributes,
        &current.roles as &[UserRole],
        current.default_currency,
    )
    .execute(conn)
    .await?;
    Ok(())
}

/// Locks the user row for the rest of the transaction and records its
/// current state in `user_history` before the caller changes it.
async fn lock_and_snapshot(conn: &mut PgConnection, id: Uuid) -> Result<User> {
    let current = query_user!(select "WHERE id = $1 FOR UPDATE", id)
        .fetch_one(&mut *conn)
        .await?;
    record_user_history(conn, &current).await?;
    Ok(current)
}

/// Retrieves user specified by id
#[tracing::instrument(skip(executor))]
pub async fn get_user_by_id<'e, E>(executor: E, id: Uuid) -> Result<User>
where
    E: PgExecutor<'e>,
{
    let user = query_user!(select "WHERE id = $1", id)
        .fetch_one(executor)
        .await?;

    Ok(user)
}

/// Retrieves user specified by email
#[tracing::instrument(skip(executor))]
pub async fn get_user_by_email<'e, E>(executor: E, email: &str) -> Result<User>
where
    E: PgExecutor<'e>,
{
    let user = query_user!(select "WHERE email = $1", email)
        .fetch_one(executor)
        .await?;

    Ok(user)
}

#[tracing::instrument(skip(pool))]
pub async fn update_user(pool: &PgPool, id: Uuid, updated_user: &UpdatedUser) -> Result<User> {
    let mut tx = pool.begin().await?;
    lock_and_snapshot(&mut tx, id).await?;

    if let Some(new_currency) = updated_user.default_currency.as_deref() {
        let current =
            sqlx::query_scalar!(r#"SELECT default_currency FROM "user" WHERE id = $1"#, id)
                .fetch_one(&mut *tx)
                .await?;
        // An unchanged (even inactive) currency is left alone.
        if current != new_currency {
            crate::currency::ensure_active(&mut tx, new_currency).await?;
        }
    }

    let updated = query_user!(
        returning
        r#"
        UPDATE "user"
        SET
            email = COALESCE($2, email),
            password_hash = COALESCE($3, password_hash),
            first_name = COALESCE($4, first_name),
            last_name = COALESCE($5, last_name),
            phone_number = COALESCE($6, phone_number),
            is_active = COALESCE($7, is_active),
            is_verified = COALESCE($8, is_verified),
            verification_code = COALESCE($9, verification_code),
            verification_code_expires_at = COALESCE($10, verification_code_expires_at),
            attributes = COALESCE($11, attributes),
            roles = COALESCE($12, roles),
            default_currency = COALESCE($13, default_currency),
            slug = COALESCE($14, slug),
            updated_at = now()
        WHERE id = $1"#,
        id,
        updated_user.email,
        updated_user.password_hash,
        updated_user.first_name,
        updated_user.last_name,
        updated_user.phone_number,
        updated_user.is_active,
        updated_user.is_verified,
        updated_user.verification_code,
        updated_user.verification_code_expires_at,
        updated_user.attributes,
        updated_user.roles.as_deref() as Option<&[UserRole]>,
        updated_user.default_currency,
        updated_user.slug,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(slug_taken)?;

    tx.commit().await?;

    Ok(updated)
}

pub async fn create_booker_profile<'e, E>(
    executor: E,
    user_id: Uuid,
    profile: &NewBookerProfile,
) -> Result<BookerProfile>
where
    E: PgExecutor<'e>,
{
    let profile = sqlx::query_as!(
        BookerProfile,
        r#"
        INSERT INTO booker_profiles (user_id, emergency_contacts, booking_preferences, loyalty)
        VALUES ($1, $2, $3, $4)
        RETURNING user_id, emergency_contacts, booking_preferences, loyalty, created_at, updated_at
        "#,
        user_id,
        profile.emergency_contacts,
        profile.booking_preferences,
        profile.loyalty
    )
    .fetch_one(executor)
    .await?;

    Ok(profile)
}

pub async fn create_host_profile<'e, E>(
    executor: E,
    user_id: Uuid,
    profile: &NewHostProfile,
) -> Result<HostProfile>
where
    E: PgExecutor<'e>,
{
    let profile = sqlx::query_as!(
        HostProfile,
        r#"
        INSERT INTO host_profiles (user_id, verified_status, payout_details, description)
        VALUES ($1, $2, $3, $4)
        RETURNING user_id, verified_status, payout_details, description, created_at, updated_at
        "#,
        user_id,
        profile.verified_status,
        profile.payout_details,
        profile.description
    )
    .fetch_one(executor)
    .await?;

    Ok(profile)
}

#[tracing::instrument(skip(pool))]
pub async fn complete_user_verification(pool: &PgPool, id: Uuid) -> Result<User> {
    let mut tx = pool.begin().await?;
    lock_and_snapshot(&mut tx, id).await?;

    let updated = query_user!(
        returning
        r#"
        UPDATE "user"
        SET
            is_verified = TRUE,
            verification_code = NULL,
            verification_code_expires_at = NULL,
            updated_at = now()
        WHERE id = $1"#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(updated)
}

#[tracing::instrument(skip(pool))]
pub async fn regenerate_verification_code(
    pool: &PgPool,
    email: &str,
    new_otp: &str,
    expiry: chrono::DateTime<chrono::Utc>,
) -> Result<Option<User>> {
    let mut tx = pool.begin().await?;

    let current = query_user!(select "WHERE email = $1 FOR UPDATE", email)
        .fetch_optional(&mut *tx)
        .await?;

    if let Some(current) = current {
        record_user_history(&mut tx, &current).await?;

        let updated = query_user!(
            returning
            r#"
            UPDATE "user"
            SET
                verification_code = $2,
                verification_code_expires_at = $3,
                updated_at = now()
            WHERE id = $1"#,
            current.id,
            new_otp,
            expiry,
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(Some(updated))
    } else {
        tx.rollback().await?;
        Ok(None)
    }
}

/// Retrieves all users with optional filtering and pagination
#[tracing::instrument(skip(executor))]
pub async fn get_all_users<'e, E>(
    executor: E,
    page: u32,
    per_page: u32,
    filter: Option<String>,
    is_deleted: Option<bool>,
) -> Result<Vec<User>>
where
    E: PgExecutor<'e>,
{
    let offset = (page - 1) * per_page;

    let mut query_builder = sqlx::QueryBuilder::new(
        r#"
        SELECT id, email, password_hash, first_name, last_name, phone_number, is_active, is_verified, verification_code, verification_code_expires_at, created_at, updated_at, attributes, roles, default_currency, deleted_at, slug
        FROM "user"
        WHERE 1 = 1
        "#,
    );

    match is_deleted {
        Some(true) => {
            query_builder.push(" AND deleted_at IS NOT NULL ");
        }
        Some(false) => {
            query_builder.push(" AND deleted_at IS NULL ");
        }
        None => {
            // No filter on deleted_at: returns all users (active and archived)
        }
    }

    if let Some(search) = filter {
        let pattern = format!("%{}%", search);
        query_builder.push(" AND (email ILIKE ");
        query_builder.push_bind(pattern.clone());
        query_builder.push(" OR first_name ILIKE ");
        query_builder.push_bind(pattern.clone());
        query_builder.push(" OR last_name ILIKE ");
        query_builder.push_bind(pattern);
        query_builder.push(" ) ");
    }

    query_builder.push(" ORDER BY created_at DESC ");
    query_builder.push(" LIMIT ");
    query_builder.push_bind(per_page as i64);
    query_builder.push(" OFFSET ");
    query_builder.push_bind(offset as i64);

    let users = query_builder
        .build_query_as::<User>()
        .fetch_all(executor)
        .await?;

    Ok(users)
}

/// Shortest password accepted for the bootstrap admin account.
pub const MIN_SYSTEM_ADMIN_PASSWORD_LEN: usize = 12;

/// Creates the system admin account if it doesn't exist yet.
///
/// `password` is only needed on first boot against an empty database; once
/// the admin exists it is ignored, so rotating it means changing the
/// password in the app, not the variable. Any failure is returned so the
/// service refuses to start rather than running without an admin. Losing a
/// creation race to another instance starting at the same time is not a
/// failure.
#[tracing::instrument(skip(pool, password))]
pub async fn initialize_system_admin(pool: &PgPool, password: Option<&str>) -> anyhow::Result<()> {
    use anyhow::Context;

    let admin_email = "admin@ourplaces.io";

    let exists = sqlx::query_scalar!(
        r#"SELECT EXISTS(SELECT 1 FROM "user" WHERE email = $1) AS "exists!""#,
        admin_email
    )
    .fetch_one(pool)
    .await
    .context("checking whether the system admin exists")?;

    if exists {
        return Ok(());
    }

    let password = password
        .filter(|p| !p.trim().is_empty())
        .context("SYSTEM_ADMIN_PASSWORD must be set to create the system admin account")?;
    if password.chars().count() < MIN_SYSTEM_ADMIN_PASSWORD_LEN {
        anyhow::bail!(
            "SYSTEM_ADMIN_PASSWORD must be at least {MIN_SYSTEM_ADMIN_PASSWORD_LEN} characters"
        );
    }

    tracing::info!("Initializing default system admin");

    let password = password.to_owned();
    let password_hash =
        tokio::task::spawn_blocking(move || bcrypt::hash(password, bcrypt::DEFAULT_COST))
            .await
            .context("joining the admin password hash task")?
            .context("hashing the system admin password")?;

    let new_user = NewUser {
        id: Uuid::now_v7(),
        email: admin_email.to_string(),
        password_hash,
        first_name: "System".to_string(),
        last_name: "Admin".to_string(),
        phone_number: None,
        is_active: true,
        is_verified: true,
        verification_code: None,
        verification_code_expires_at: None,
        attributes: serde_json::json!({}),
        roles: Some(vec![UserRole::Admin]),
        default_currency: "USD".to_string(),
    };

    match create_user(pool, &new_user).await {
        Ok(_) => {
            tracing::info!("Default system admin initialized successfully");
            Ok(())
        }
        Err(DbError::Sqlx(e))
            if e.as_database_error()
                .is_some_and(|db| db.is_unique_violation()) =>
        {
            tracing::info!("System admin was created by another instance");
            Ok(())
        }
        Err(e) => Err(e).context("creating the default system admin"),
    }
}

/// Updates a user's password and clears any pending verification code.
#[tracing::instrument(skip(pool))]
pub async fn update_user_password(
    pool: &PgPool,
    id: Uuid,
    new_password_hash: String,
) -> Result<User> {
    let mut tx = pool.begin().await?;
    lock_and_snapshot(&mut tx, id).await?;

    let updated = query_user!(
        returning
        r#"
        UPDATE "user"
        SET
            password_hash = $2,
            verification_code = NULL,
            verification_code_expires_at = NULL,
            updated_at = now()
        WHERE id = $1"#,
        id,
        new_password_hash,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(updated)
}

/// Updates a user's email, resets verification status, and sets a new verification code.
#[tracing::instrument(skip(pool))]
pub async fn update_user_email(
    pool: &PgPool,
    id: Uuid,
    new_email: String,
    verification_code: String,
    expiry: chrono::DateTime<chrono::Utc>,
) -> Result<User> {
    let mut tx = pool.begin().await?;
    lock_and_snapshot(&mut tx, id).await?;

    let updated = query_user!(
        returning
        r#"
        UPDATE "user"
        SET
            email = $2,
            is_verified = FALSE,
            verification_code = $3,
            verification_code_expires_at = $4,
            updated_at = now()
        WHERE id = $1"#,
        id,
        new_email,
        verification_code,
        expiry,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(updated)
}

/// Deactivates a user's account.
#[tracing::instrument(skip(pool))]
pub async fn deactivate_user(pool: &PgPool, id: Uuid) -> Result<User> {
    let mut tx = pool.begin().await?;
    lock_and_snapshot(&mut tx, id).await?;

    let updated = query_user!(
        returning
        r#"
        UPDATE "user"
        SET
            is_active = FALSE,
            updated_at = now()
        WHERE id = $1"#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(updated)
}

/// Soft deletes a user by setting deleted_at to current timestamp.
#[tracing::instrument(skip(pool))]
pub async fn soft_delete_user(pool: &PgPool, id: Uuid) -> Result<User> {
    let mut tx = pool.begin().await?;
    lock_and_snapshot(&mut tx, id).await?;

    let updated = query_user!(
        returning
        r#"
        UPDATE "user"
        SET
            is_active = FALSE,
            deleted_at = now(),
            updated_at = now()
        WHERE id = $1"#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(updated)
}

/// Restores a soft-deleted user by setting deleted_at to NULL.
#[tracing::instrument(skip(pool))]
pub async fn restore_user(pool: &PgPool, id: Uuid) -> Result<User> {
    let mut tx = pool.begin().await?;
    lock_and_snapshot(&mut tx, id).await?;

    let updated = query_user!(
        returning
        r#"
        UPDATE "user"
        SET
            is_active = TRUE,
            deleted_at = NULL,
            updated_at = now()
        WHERE id = $1"#,
        id,
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(updated)
}

/// Permanently hard deletes a user from the database.
#[tracing::instrument(skip(pool))]
pub async fn hard_delete_user(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query!(r#"DELETE FROM "user" WHERE id = $1"#, id)
        .execute(pool)
        .await
        .map_err(|err| {
            if let Some(db_err) = err.as_database_error()
                && matches!(
                    db_err.code().as_deref(),
                    Some(crate::error::SQLSTATE_FOREIGN_KEY_VIOLATION | crate::error::SQLSTATE_RESTRICT_VIOLATION)
                )
            {
                return DbError::ValidationError(
                    "Cannot permanently delete user with linked bookings, listings, promo codes, or financial records. Please use Soft Delete (Archive) instead.".to_string(),
                );
            }
            DbError::Sqlx(err)
        })?;
    Ok(())
}

#[cfg(test)]
mod slug_tests {
    use super::*;
    use crate::models::NewUser;
    use sqlx::postgres::PgPoolOptions;

    async fn pool() -> PgPool {
        dotenvy::dotenv().ok();
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/our_places".into());
        PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("Failed to connect to Postgres")
    }

    async fn user(pool: &PgPool, first: &str, last: &str) -> User {
        let id = Uuid::now_v7();
        create_user(
            pool,
            &NewUser {
                id,
                email: format!("slug_{id}@example.com"),
                password_hash: "secret".into(),
                first_name: first.into(),
                last_name: last.into(),
                phone_number: None,
                is_active: true,
                is_verified: true,
                verification_code: None,
                verification_code_expires_at: None,
                attributes: serde_json::json!({}),
                roles: None,
                default_currency: "USD".into(),
            },
        )
        .await
        .expect("create user")
    }

    async fn cleanup(pool: &PgPool, ids: &[Uuid]) {
        let _ = sqlx::query("DELETE FROM \"user\" WHERE id = ANY($1)")
            .bind(ids)
            .execute(pool)
            .await;
    }

    #[tokio::test]
    async fn new_users_get_unique_first_last_slugs() {
        let pool = pool().await;
        // A random surname keeps reruns independent of other test data.
        let last = format!("Slugtest{}", Uuid::now_v7().simple());
        let a = user(&pool, "Zoë", &last).await;
        let b = user(&pool, "Zoë", &last).await;
        let expected = common::slug::slugify(&format!("zoe-{last}"));
        assert_eq!(a.slug, expected);
        assert_eq!(b.slug, format!("{expected}-2"));
        cleanup(&pool, &[a.id, b.id]).await;
    }

    #[tokio::test]
    async fn a_slug_can_be_changed_but_not_to_a_taken_one() {
        let pool = pool().await;
        let tag = Uuid::now_v7().simple().to_string();
        let a = user(&pool, "Ann", &format!("Slugone{tag}")).await;
        let b = user(&pool, "Ben", &format!("Slugtwo{tag}")).await;
        let wanted = format!("ann-villas-{tag}");

        let renamed = update_user(
            &pool,
            a.id,
            &UpdatedUser {
                slug: Some(wanted.clone()),
                ..Default::default()
            },
        )
        .await
        .expect("rename");
        assert_eq!(renamed.slug, wanted);

        // A name change keeps the slug, so links keep working.
        let renamed_again = update_user(
            &pool,
            a.id,
            &UpdatedUser {
                first_name: Some("Annie".into()),
                ..Default::default()
            },
        )
        .await
        .expect("name change");
        assert_eq!(renamed_again.slug, wanted);

        let clash = update_user(
            &pool,
            b.id,
            &UpdatedUser {
                slug: Some(wanted.clone()),
                ..Default::default()
            },
        )
        .await;
        assert!(
            matches!(&clash, Err(DbError::ValidationError(m)) if m.contains("already taken")),
            "{clash:?}"
        );
        cleanup(&pool, &[a.id, b.id]).await;
    }
}
