use thiserror::Error;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("Database error")]
    Sqlx(#[from] sqlx::Error),
}

// A custom `Result` type that defaults to using our `DbError`.
pub type Result<T> = std::result::Result<T, DbError>;
