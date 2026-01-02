use crate::error::Result;
use crate::models::{NewUser, UpdatedUser, User};
use chrono::Utc;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

/// Creates a new user in the database.
#[tracing::instrument(skip(executor))]
pub async fn create_user<'e, E>(executor: E, new_user_request: &NewUser) -> Result<User>
where
    E: PgExecutor<'e>,
{
    let user = sqlx::query_as!(
        User,
        r#"
            INSERT INTO "user" (id, email, password_hash, first_name, last_name, phone_number, is_active, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
        "#,
        new_user_request.id,
        new_user_request.email,
        new_user_request.password_hash,
        new_user_request.first_name,
        new_user_request.last_name,
        new_user_request.phone_number,
        new_user_request.is_active,
        Utc::now(),
        Utc::now(),
    )
    .fetch_one(executor)
    .await?;

    Ok(user)
}

/// Retrieves user specified by id
#[tracing::instrument(skip(executor))]
pub async fn get_user_by_id<'e, E>(executor: E, id: Uuid) -> Result<User>
where
    E: PgExecutor<'e>,
{
    let user = sqlx::query_as!(
        User,
        r#"
            SELECT * FROM "user" WHERE id = $1
        "#,
        id,
    )
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
    let user = sqlx::query_as!(
        User,
        r#"
            SELECT * FROM "user" WHERE email = $1
        "#,
        email,
    )
    .fetch_one(executor)
    .await?;

    Ok(user)
}

#[tracing::instrument(skip(pool))]
pub async fn update_user(pool: &PgPool, id: Uuid, updated_user: &UpdatedUser) -> Result<User> {
    let mut tx = pool.begin().await?;

    let current = sqlx::query_as!(User, r#"SELECT * FROM "user" WHERE id = $1 FOR UPDATE"#, id)
        .fetch_one(&mut *tx)
        .await?;

    sqlx::query!(
        r#"
        INSERT INTO user_history
        (user_id, email, password_hash, first_name, last_name, phone_number, is_active, valid_from)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
        current.id,
        current.email,
        current.password_hash,
        current.first_name,
        current.last_name,
        current.phone_number,
        current.is_active,
        current.updated_at
    )
    .execute(&mut *tx)
    .await?;

    let updated = sqlx::query_as!(
        User,
        r#"
        UPDATE "user"
        SET
            email = COALESCE($2, email),
            password_hash = COALESCE($3, password_hash),
            first_name = COALESCE($4, first_name),
            last_name = COALESCE($5, last_name),
            phone_number = COALESCE($6, phone_number),
            is_active = COALESCE($7, is_active),
            updated_at = now()
        WHERE id = $1
        RETURNING *
        "#,
        id,
        updated_user.email,
        updated_user.password_hash,
        updated_user.first_name,
        updated_user.last_name,
        updated_user.phone_number,
        updated_user.is_active
    )
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(updated)
}
