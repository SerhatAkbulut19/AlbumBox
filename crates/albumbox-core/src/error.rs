use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(&'static str),
    #[error("invalid input: {0}")]
    Invalid(&'static str),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type CoreResult<T> = Result<T, CoreError>;
