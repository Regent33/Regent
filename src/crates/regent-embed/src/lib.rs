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
use std::path::PathBuf;
use std::sync::Mutex;

/// Default model: all-MiniLM-L6-v2 (384-dim) — small, fast, strong on short
/// semantic-similarity tasks (exactly the memory-recall shape).
const MODEL_ID: &str = "all-MiniLM-L6-v2";
const MODEL_DIM: usize = 384;

/// Where the weights are cached: `$REGENT_HOME/models/fastembed`, beside the
/// speech models' `$REGENT_HOME/models` root.
///
/// Not fastembed's default, which is `./.fastembed_cache` — relative to the
/// *working directory*. That dropped ~87MB into whichever directory happened to
/// be current: the installer's payload staging dir at build time (one build
/// away from shipping it), and the app's own install directory at runtime,
/// since the desktop app is launched with its cwd set there. Installed somewhere
/// only an administrator can write — now that Setup allows `D:\Program Files` —
/// a normal user could not create it at all and embedding failed outright.
///
/// A downloaded model is user data. It belongs with the user's other data, at a
/// path that does not depend on where anyone happened to be standing.
fn cache_dir() -> PathBuf {
    let home = std::env::var("REGENT_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let user = std::env::var("USERPROFILE")
                .or_else(|_| std::env::var("HOME"))
                .unwrap_or_default();
            PathBuf::from(user).join(".regent")
        });
    home.join("models").join("fastembed")
}

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
        let cache = cache_dir();
        let model = TextEmbedding::try_new(
            InitOptions::new(EmbeddingModel::AllMiniLML6V2).with_cache_dir(cache.clone()),
        )
        .map_err(|e| RegentError::Provider(format!("embedding model init: {e}")))?;
        tracing::info!(
            model = MODEL_ID,
            dim = MODEL_DIM,
            cache = %cache.display(),
            "local embedding model ready"
        );
        Ok(Self {
            model: Mutex::new(model),
        })
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

    /// Guards the actual bug: fastembed's default cache is `./.fastembed_cache`,
    /// so dropping `with_cache_dir` silently writes ~87MB wherever the process
    /// happens to be standing — which, for the installed desktop app, is its own
    /// program directory. Runs offline, unlike the model test below.
    #[test]
    fn cache_is_not_relative_to_the_working_directory() {
        let dir = cache_dir();
        assert!(
            dir.ends_with("models/fastembed"),
            "cache must sit under $REGENT_HOME/models, got {dir:?}"
        );
        // The failure mode is a *relative* path resolved against the cwd; the
        // fallback only yields one if neither HOME nor USERPROFILE is set.
        if std::env::var_os("REGENT_HOME").is_some()
            || std::env::var_os("HOME").is_some()
            || std::env::var_os("USERPROFILE").is_some()
        {
            assert!(dir.is_absolute(), "cwd-relative cache: {dir:?}");
        }
    }

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
