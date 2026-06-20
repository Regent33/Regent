//! The embedding contract — the semantic lane of memory retrieval. The memory
//! layer depends only on this; concrete generators (local ONNX, a remote API)
//! are infrastructure injected at the composition root.

use crate::types::error::RegentError;

pub trait EmbeddingProvider: Send + Sync {
    /// Embeds a batch of texts. Batching amortises model setup/inference cost;
    /// returns one vector per input, each of length [`dim`](Self::dim).
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, RegentError>;

    /// Stable identifier of the model. Stored vectors are keyed by this so a
    /// model swap never silently mixes incompatible embedding spaces.
    fn model_id(&self) -> &str;

    /// Embedding dimensionality.
    fn dim(&self) -> usize;
}
