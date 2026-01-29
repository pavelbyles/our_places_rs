use actix_session::storage::{LoadError, SaveError, SessionKey, SessionStore, UpdateError};
use actix_web::cookie::time::Duration;
use anyhow::Error;
use async_trait::async_trait;
use db_core::sessions::SessionsDb;
use std::collections::HashMap;

#[derive(Clone)]
pub struct AdminSessionStore {
    inner: SessionsDb,
}

impl AdminSessionStore {
    pub fn new(inner: SessionsDb) -> Self {
        Self { inner }
    }
}

#[async_trait(?Send)]
impl SessionStore for AdminSessionStore {
    async fn load(
        &self,
        session_key: &SessionKey,
    ) -> Result<Option<HashMap<String, String>>, LoadError> {
        let id = session_key.as_ref();
        let state_bytes = self
            .inner
            .load(id)
            .await
            .map_err(|e| LoadError::Other(e.into()))?;

        if let Some(bytes) = state_bytes {
            let state: HashMap<String, String> =
                serde_json::from_slice(&bytes).map_err(|e| LoadError::Deserialization(e.into()))?;
            return Ok(Some(state));
        }

        Ok(None)
    }

    async fn save(
        &self,
        session_state: HashMap<String, String>,
        ttl: &Duration,
    ) -> Result<SessionKey, SaveError> {
        let state = session_state;
        let bytes = serde_json::to_vec(&state).map_err(|e| SaveError::Serialization(e.into()))?;

        let id = uuid::Uuid::new_v4().to_string();
        let ttl_seconds = ttl.whole_seconds();

        self.inner
            .save(&id, &bytes, ttl_seconds)
            .await
            .map_err(|e| SaveError::Other(e.into()))?;

        Ok(id.try_into().unwrap())
    }

    async fn update(
        &self,
        session_key: SessionKey,
        session_state: HashMap<String, String>,
        ttl: &Duration,
    ) -> Result<SessionKey, UpdateError> {
        let state = session_state;
        let bytes = serde_json::to_vec(&state).map_err(|e| UpdateError::Serialization(e.into()))?;
        let id = session_key.as_ref();
        let ttl_seconds = ttl.whole_seconds();

        self.inner
            .save(id, &bytes, ttl_seconds)
            .await
            .map_err(|e| UpdateError::Other(e.into()))?;

        Ok(session_key)
    }

    async fn update_ttl(&self, session_key: &SessionKey, ttl: &Duration) -> Result<(), Error> {
        // No-op for now, as save/update handles it
        Ok(())
    }

    async fn delete(&self, session_key: &SessionKey) -> Result<(), Error> {
        self.inner
            .delete(session_key.as_ref())
            .await
            .map_err(|e| e.into())
    }
}
