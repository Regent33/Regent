//! regent-embed — local ONNX text embeddings (canonical `shared/infrastructure`).
//!
//! Wraps `fastembed` (ONNX Runtime + all-MiniLM-L6-v2, 384-dim) behind the
//! kernel `EmbeddingProvider` contract. Runs fully offline after a one-time
//! model download into the fastembed cache; zero per-query cost, no network at
//! inference, no PII leaving the machine. The memory layer never sees this
//! crate — only the trait — so the model is swappable from the composition
//! root.

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use regent_kernel::{EmbeddingProvider, RegentError};
use std::sync::Mutex;

/// Default model: all-MiniLM-L6-v2 (384-dim) — small, fast, strong on short
/// semantic-similarity tasks (exactly the memory-recall shape).
const MODEL_ID: &str = "all-MiniLM-L6-v2";
const MODEL_DIM: usize = 384;

pub struct FastEmbedProvider {
    /// `TextEmbedding::embed` takes `&mut self` (it mutates the ONNX session),
    /// so a single instance is serialised behind a mutex. Embedding is
    /// CPU-bound and batched internally — one model instance is the right
    /// footprint for a personal agent.
    model: Mutex<TextEmbedding>,
}

impl FastEmbedProvider {
    /// Loads the model, downloading weights into the fastembed cache on first
    /// use. Subsequent runs are offline.
    pub fn new() -> Result<Self, RegentError> {
        let model = TextEmbedding::try_new(InitOptions::new(EmbeddingModel::AllMiniLML6V2))
            .map_err(|e| RegentError::Provider(format!("embedding model init: {e}")))?;
        tracing::info!(model = MODEL_ID, dim = MODEL_DIM, "local embedding model ready");
        Ok(Self { model: Mutex::new(model) })
    }
}

impl EmbeddingProvider for FastEmbedProvider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, RegentError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let documents: Vec<&str> = texts.iter().map(String::as_str).collect();
        self.model
            .lock()
            .map_err(|_| RegentError::Provider("embedding model lock poisoned".into()))?
            .embed(documents, None)
            .map_err(|e| RegentError::Provider(format!("embedding inference: {e}")))
    }

    fn model_id(&self) -> &str {
        MODEL_ID
    }

    fn dim(&self) -> usize {
        MODEL_DIM
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real end-to-end check: downloads the model (~90 MB) on first run and
    /// runs ONNX inference. Ignored by default so the offline test suite stays
    /// fast — run explicitly: `cargo test -p regent-embed -- --ignored`.
    #[test]
    #[ignore = "downloads the ONNX model and runs inference (network + ~90MB)"]
    fn embeds_real_text_with_semantic_similarity() {
        let provider = FastEmbedProvider::new().expect("model loads");
        let vectors = provider
            .embed(&[
                "the cat sat on the mat".to_owned(),
                "a feline rested on the rug".to_owned(),
                "rust async runtime internals".to_owned(),
            ])
            .expect("embedding succeeds");

        assert_eq!(vectors.len(), 3);
        assert_eq!(vectors[0].len(), MODEL_DIM);

        let cos = |a: &[f32], b: &[f32]| -> f32 {
            let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
            let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
            dot / (na * nb)
        };
        // Paraphrase pair should out-score the unrelated pair — the exact win
        // FTS5 misses and the vector lane captures.
        let paraphrase = cos(&vectors[0], &vectors[1]);
        let unrelated = cos(&vectors[0], &vectors[2]);
        assert!(
            paraphrase > unrelated,
            "paraphrase {paraphrase:.3} should beat unrelated {unrelated:.3}"
        );
    }
}
