use crate::domain::contracts::{PersistenceBackend, VectorStore};
use crate::domain::entities::VectorRecord;
use crate::domain::errors::CoreError;
use serde_json::Value;
use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap};
use std::sync::Arc;
use tokio::sync::RwLock;

type StoredVector = (Vec<f32>, Value);
type VectorStoreMap = HashMap<String, StoredVector>;

#[derive(Debug, Clone, Default)]
pub struct InMemoryPersistenceBackend {
    store: Arc<RwLock<HashMap<String, Value>>>,
}

impl InMemoryPersistenceBackend {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl PersistenceBackend for InMemoryPersistenceBackend {
    async fn save_state(&self, scope: &str, state: Value) -> Result<(), CoreError> {
        self.store.write().await.insert(scope.to_owned(), state);
        Ok(())
    }

    async fn load_state(&self, scope: &str) -> Result<Option<Value>, CoreError> {
        Ok(self.store.read().await.get(scope).cloned())
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryVectorStore {
    store: Arc<RwLock<VectorStoreMap>>,
}

impl InMemoryVectorStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl VectorStore for InMemoryVectorStore {
    async fn upsert(&self, id: &str, vector: Vec<f32>, metadata: Value) -> Result<(), CoreError> {
        self.store
            .write()
            .await
            .insert(id.to_owned(), (vector, metadata));
        Ok(())
    }

    async fn query(&self, vector: Vec<f32>, limit: usize) -> Result<Vec<VectorRecord>, CoreError> {
        let store = self.store.read().await;
        if limit == 0 {
            return Ok(Vec::new());
        }
        // Maintain a min-heap of size `limit` so we never materialize the
        // full sorted record list. For N candidates and a top-k of `limit`,
        // this is O(N log limit) vs the previous O(N log N), and only
        // clones the metadata of records that make it into the heap.
        let mut heap: BinaryHeap<Reverse<HeapEntry<'_>>> = BinaryHeap::with_capacity(limit + 1);
        for (id, (candidate, metadata)) in store.iter() {
            let score = cosine_similarity(&vector, candidate);
            heap.push(Reverse(HeapEntry {
                id,
                metadata,
                score,
            }));
            if heap.len() > limit {
                heap.pop();
            }
        }
        // `into_sorted_vec` on `Reverse<_>` sorts ascending by `Reverse`,
        // which is descending by the underlying `HeapEntry::cmp` — i.e.
        // highest-score records first. Exactly the order we want.
        let records = heap
            .into_sorted_vec()
            .into_iter()
            .map(|Reverse(entry)| VectorRecord {
                id: entry.id.clone(),
                score: entry.score,
                metadata: entry.metadata.clone(),
            })
            .collect::<Vec<_>>();
        Ok(records)
    }
}

#[derive(Debug)]
struct HeapEntry<'a> {
    id: &'a String,
    metadata: &'a Value,
    score: f32,
}

impl PartialEq for HeapEntry<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score && self.id == other.id
    }
}

impl Eq for HeapEntry<'_> {}

impl PartialOrd for HeapEntry<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for HeapEntry<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        // NaN scores sort as equal-and-low so they don't poison the heap.
        self.score
            .partial_cmp(&other.score)
            .unwrap_or(Ordering::Equal)
            // Tie-break on id so the result is deterministic across runs.
            .then_with(|| self.id.cmp(other.id))
    }
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }

    let dot = left.iter().zip(right).map(|(l, r)| l * r).sum::<f32>();
    let left_norm = left.iter().map(|value| value * value).sum::<f32>().sqrt();
    let right_norm = right.iter().map(|value| value * value).sum::<f32>().sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm * right_norm)
    }
}
