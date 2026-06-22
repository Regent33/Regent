//! regent-speech — the pluggable voice stack (canonical `shared/infrastructure`).
//!
//! Houses the ASR/TTS provider **registry**, the model manager, VAD, and the
//! ASR robustness layer. Concrete backends (local Qwen3, remote Whisper /
//! ElevenLabs, a shell `command` provider) implement the kernel
//! [`AsrProvider`](regent_kernel::AsrProvider) /
//! [`TtsProvider`](regent_kernel::TtsProvider) contracts; the registry and
//! manager are model-agnostic. The whole subsystem is **disabled by default**
//! (`speech.enabled: false`) — nothing here loads or downloads a model until
//! `regent voice setup` turns it on.

pub mod infra;
pub mod models;
pub mod registry;
pub mod robustness;
pub mod vad;
pub mod wav;

pub use infra::remote::{
    HttpBody, HttpExecutor, OpenAiCompatAsr, OpenAiCompatTts, SpeechHttpRequest,
    build_speech_request, build_transcription_request, parse_transcription_response,
};
pub use models::{ManagerError, ModelFile, ModelKind, ModelManager, ModelSpec, sha256_hex};
pub use registry::{BUILTIN_ASR_PROVIDERS, BUILTIN_TTS_PROVIDERS, ProviderRegistry, RegistryError};
pub use robustness::{chunk_ranges, clean_transcript, is_hallucination};
pub use vad::{Vad, VadConfig, rms};
