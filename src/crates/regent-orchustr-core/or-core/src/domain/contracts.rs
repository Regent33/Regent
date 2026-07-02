#![allow(async_fn_in_trait)]

use crate::domain::entities::VectorRecord;
use crate::domain::errors::CoreError;
use serde_json::Value;
use std::collections::HashMap;

pub type DynState = HashMap<String, Value>;

pub trait OrchState:
    Clone + Send + Sync + serde::Serialize + serde::de::DeserializeOwned + 'static
{
    /// Merges `patch` into `current`, returning the combined state.
    ///
    /// # Default strategy
    ///
    /// The default implementation uses a **full-replacement** strategy:
    /// `current` is discarded and `patch` is returned as-is.  Override this
    /// method to implement custom merge logic (e.g., key-level deep merge).
    fn merge(current: &Self, patch: Self) -> Self {
        let _ = current; // Default: full replacement strategy
        patch
    }
}

impl OrchState for DynState {
    /// Key-level merge: all keys from `patch` are inserted into a clone of
    /// `current`.  Patch keys take precedence on conflict.
    fn merge(current: &Self, patch: Self) -> Self {
        let mut merged = current.clone();
        for (key, value) in patch {
            merged.insert(key, value);
        }
        merged
    }
}

#[cfg_attr(test, mockall::automock)]
pub trait PersistenceBackend: Send + Sync + 'static {
    async fn save_state(&self, scope: &str, state: Value) -> Result<(), CoreError>;
    async fn load_state(&self, scope: &str) -> Result<Option<Value>, CoreError>;
}

#[cfg_attr(test, mockall::automock)]
pub trait VectorStore: Send + Sync + 'static {
    async fn upsert(&self, id: &str, vector: Vec<f32>, metadata: Value) -> Result<(), CoreError>;
    async fn query(&self, vector: Vec<f32>, limit: usize) -> Result<Vec<VectorRecord>, CoreError>;
}
