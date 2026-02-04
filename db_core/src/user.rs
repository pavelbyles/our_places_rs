use crate::error::Result;
use crate::models::{
    BookerProfile, HostProfile, NewBookerProfile, NewHostProfile, NewUser, UpdatedUser, User,
    UserRole,
};
use chrono::Utc;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

/// Creates a new user in the database.
#[tracing::instrument(skip(executor))]
pub async fn create_user<'e, E>(executor: E, new_user_request: &NewUser) -> Result<User>
where
    E: PgExecutor<'e>,
{
    let user = sqlx::query_as::<_, User>(
        r#"
            INSERT INTO "user" (id, email, password_hash, first_name, last_name, phone_number, is_active, created_at, updated_at, attributes, roles)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id, email, password_hash, first_name, last_name, phone_number, is_active, created_at, updated_at, attributes, roles
        "#,
    )
    .bind(new_user_request.id)
    .bind(&new_user_request.email)
    .bind(&new_user_request.password_hash)
    .bind(&new_user_request.first_name)
    .bind(&new_user_request.last_name)
    .bind(&new_user_request.phone_number)
    .bind(new_user_request.is_active)
    .bind(Utc::now())
    .bind(Utc::now())
    .bind(&new_user_request.attributes)
    .bind(new_user_request.roles.clone().unwrap_or_default())
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
            SELECT id, email, password_hash, first_name, last_name, phone_number, is_active, created_at, updated_at, attributes, roles as "roles: Vec<UserRole>"
            FROM "user" WHERE id = $1
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
            SELECT id, email, password_hash, first_name, last_name, phone_number, is_active, created_at, updated_at, attributes, roles as "roles: Vec<UserRole>"
            FROM "user" WHERE email = $1
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

    let current = sqlx::query_as!(User, r#"SELECT id, email, password_hash, first_name, last_name, phone_number, is_active, created_at, updated_at, attributes, roles as "roles: Vec<UserRole>" FROM "user" WHERE id = $1 FOR UPDATE"#, id)
        .fetch_one(&mut *tx)
        .await?;

    sqlx::query(
        r#"
        INSERT INTO user_history
        (user_id, email, password_hash, first_name, last_name, phone_number, is_active, valid_from, attributes, roles)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        "#,
    )
    .bind(current.id)
    .bind(&current.email)
    .bind(&current.password_hash)
    .bind(&current.first_name)
    .bind(&current.last_name)
    .bind(&current.phone_number)
    .bind(current.is_active)
    .bind(current.updated_at)
    .bind(&current.attributes)
    .bind(&current.roles)
    .execute(&mut *tx)
    .await?;

    let updated = sqlx::query_as::<_, User>(
        r#"
        UPDATE "user"
        SET
            email = COALESCE($2, email),
            password_hash = COALESCE($3, password_hash),
            first_name = COALESCE($4, first_name),
            last_name = COALESCE($5, last_name),
            phone_number = COALESCE($6, phone_number),
            is_active = COALESCE($7, is_active),
            attributes = COALESCE($8, attributes),
            roles = COALESCE($9, roles),
            updated_at = now()
        WHERE id = $1
        RETURNING id, email, password_hash, first_name, last_name, phone_number, is_active, created_at, updated_at, attributes, roles
        "#,
    )
    .bind(id)
    .bind(&updated_user.email)
    .bind(&updated_user.password_hash)
    .bind(&updated_user.first_name)
    .bind(&updated_user.last_name)
    .bind(&updated_user.phone_number)
    .bind(updated_user.is_active)
    .bind(&updated_user.attributes)
    .bind(updated_user.roles.as_deref())
    .fetch_one(&mut *tx)
    .await?;

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
