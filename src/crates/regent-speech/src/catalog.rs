//! The speech provider catalog — **one table, many endpoints**.
//!
//! Every entry here speaks the same wire: `POST {base}/audio/transcriptions`
//! (multipart `file` + `model`) for ASR and `POST {base}/audio/speech` (JSON
//! `model`/`input`/`voice`) for TTS. That is why adding a provider is adding a
//! ROW, not an adapter — [`OpenAiCompatAsr`](crate::OpenAiCompatAsr) and
//! [`OpenAiCompatTts`](crate::OpenAiCompatTts) already serve all of them.
//!
//! A provider that does NOT speak this wire (ElevenLabs' `xi-api-key` +
//! `/text-to-speech/{voice}`, Deepgram's `/listen` with a raw body) is
//! deliberately absent rather than listed and broken: those need custom headers
//! and a raw-bytes body, which [`SpeechHttpRequest`](crate::SpeechHttpRequest)
//! cannot express today. The table is the honest answer to "what can I pick?" —
//! `voice.models` is generated from it, so the menu can never advertise a
//! backend that errors on selection.
//!
//! `base_url` is a DEFAULT, not a constraint: the config's `base_url` override
//! wins for every provider (see the deacon's `speech_factory::resolve_base`), so
//! an unlisted host, a self-hosted port you moved, or Azure's per-deployment URL
//! all work without a release.

/// One selectable speech backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpeechProvider {
    /// Config value for `speech.asr.provider` / `speech.tts.provider`.
    pub id: &'static str,
    /// Human name for menus.
    pub label: &'static str,
    /// One line on what it is and what it costs the user to run.
    pub blurb: &'static str,
    /// Default OpenAI-compatible base URL. **Empty means the user must supply
    /// one** (`speech.*.base_url`): Azure, RunPod and a bring-your-own endpoint
    /// have no fixed host, so there is no default that could be right. The
    /// factory turns an empty base into an error naming the field to set,
    /// rather than building a request against a URL nobody can serve.
    pub base_url: &'static str,
    /// Env var holding the API key. `None` = a server you run (no key).
    pub key_var: Option<&'static str>,
    /// Default speech-to-text model. `None` = this backend has no ASR.
    pub asr_model: Option<&'static str>,
    /// Default text-to-speech model. `None` = this backend has no TTS.
    pub tts_model: Option<&'static str>,
    /// True for a hosted API, false for something you run yourself.
    pub hosted: bool,
}

/// Every known provider, hosted first (they need only a key), then the
/// self-hosted servers. Order is the menu order.
pub const SPEECH_PROVIDERS: &[SpeechProvider] = &[
    SpeechProvider {
        id: "groq",
        label: "Groq",
        blurb: "free Whisper speech-to-text + PlayAI voices — just a free API key (easiest)",
        base_url: "https://api.groq.com/openai/v1",
        key_var: Some("GROQ_API_KEY"),
        asr_model: Some("whisper-large-v3-turbo"),
        tts_model: Some("playai-tts"),
        hosted: true,
    },
    SpeechProvider {
        id: "openai",
        label: "OpenAI",
        blurb: "Whisper speech-to-text + spoken replies — one API key",
        base_url: "https://api.openai.com/v1",
        key_var: Some("OPENAI_API_KEY"),
        asr_model: Some("whisper-1"),
        tts_model: Some("gpt-4o-mini-tts"),
        hosted: true,
    },
    // Kept exactly as shipped: `qwen`'s model ids are the same for DashScope's
    // compatible mode and a local vLLM, and an owner config already points at
    // them. Changing a working default is not this table's job.
    SpeechProvider {
        id: "qwen",
        label: "Qwen (DashScope)",
        blurb: "Alibaba Qwen3 speech-to-text + text-to-speech — one API key",
        base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        key_var: Some("DASHSCOPE_API_KEY"),
        asr_model: Some("qwen3-asr-1.7b"),
        tts_model: Some("qwen3-tts-1.7b"),
        hosted: true,
    },
    SpeechProvider {
        id: "deepinfra",
        label: "DeepInfra",
        blurb: "Whisper speech-to-text + Kokoro voices, pay per second",
        base_url: "https://api.deepinfra.com/v1/openai",
        key_var: Some("DEEPINFRA_API_KEY"),
        asr_model: Some("openai/whisper-large-v3-turbo"),
        tts_model: Some("hexgrad/Kokoro-82M"),
        hosted: true,
    },
    SpeechProvider {
        id: "lemonfox",
        label: "Lemonfox",
        blurb: "Whisper speech-to-text + voices at a flat rate",
        base_url: "https://api.lemonfox.ai/v1",
        key_var: Some("LEMONFOX_API_KEY"),
        asr_model: Some("whisper-1"),
        tts_model: Some("tts-1"),
        hosted: true,
    },
    SpeechProvider {
        id: "siliconflow",
        label: "SiliconFlow",
        blurb: "SenseVoice speech-to-text + CosyVoice text-to-speech",
        base_url: "https://api.siliconflow.cn/v1",
        key_var: Some("SILICONFLOW_API_KEY"),
        asr_model: Some("FunAudioLLM/SenseVoiceSmall"),
        tts_model: Some("FunAudioLLM/CosyVoice2-0.5B"),
        hosted: true,
    },
    SpeechProvider {
        id: "fireworks",
        label: "Fireworks",
        blurb: "fast Whisper v3 speech-to-text (no text-to-speech)",
        base_url: "https://api.fireworks.ai/inference/v1",
        key_var: Some("FIREWORKS_API_KEY"),
        asr_model: Some("whisper-v3-turbo"),
        tts_model: None,
        hosted: true,
    },
    SpeechProvider {
        id: "together",
        label: "Together AI",
        blurb: "Whisper v3 speech-to-text (no text-to-speech)",
        base_url: "https://api.together.xyz/v1",
        key_var: Some("TOGETHER_API_KEY"),
        asr_model: Some("openai/whisper-large-v3"),
        tts_model: None,
        hosted: true,
    },
    SpeechProvider {
        id: "mistral",
        label: "Mistral",
        blurb: "Voxtral speech-to-text (no text-to-speech)",
        base_url: "https://api.mistral.ai/v1",
        key_var: Some("MISTRAL_API_KEY"),
        asr_model: Some("voxtral-mini-latest"),
        tts_model: None,
        hosted: true,
    },
    SpeechProvider {
        id: "novita",
        label: "Novita AI",
        blurb: "Whisper speech-to-text, pay per use (no text-to-speech)",
        base_url: "https://api.novita.ai/v3/openai",
        key_var: Some("NOVITA_API_KEY"),
        asr_model: Some("whisper-large-v3"),
        tts_model: None,
        hosted: true,
    },
    SpeechProvider {
        id: "sambanova",
        label: "SambaNova",
        blurb: "Whisper v3 speech-to-text on SambaNova hardware (no text-to-speech)",
        base_url: "https://api.sambanova.ai/v1",
        key_var: Some("SAMBANOVA_API_KEY"),
        asr_model: Some("Whisper-Large-v3"),
        tts_model: None,
        hosted: true,
    },
    SpeechProvider {
        id: "aimlapi",
        label: "AI/ML API",
        blurb: "aggregator — Whisper speech-to-text + several voice models on one key",
        base_url: "https://api.aimlapi.com/v1",
        key_var: Some("AIMLAPI_API_KEY"),
        asr_model: Some("whisper-large"),
        tts_model: Some("tts-1"),
        hosted: true,
    },
    // ---- endpoints with no fixed host: base_url is required, not optional ----
    SpeechProvider {
        id: "azure",
        label: "Azure OpenAI",
        blurb: "your Azure deployment — set base_url to the endpoint incl. deployment + api-version",
        base_url: "",
        key_var: Some("AZURE_OPENAI_API_KEY"),
        asr_model: Some("whisper"),
        tts_model: Some("tts"),
        hosted: true,
    },
    SpeechProvider {
        id: "runpod",
        label: "RunPod",
        blurb: "your RunPod serverless endpoint — set base_url to its OpenAI-compatible URL",
        base_url: "",
        key_var: Some("RUNPOD_API_KEY"),
        asr_model: Some("whisper-1"),
        tts_model: Some("tts-1"),
        hosted: true,
    },
    SpeechProvider {
        id: "custom",
        label: "Custom endpoint",
        blurb: "any OpenAI-compatible speech server — set base_url (key: REGENT_SPEECH_API_KEY)",
        base_url: "",
        key_var: Some("REGENT_SPEECH_API_KEY"),
        asr_model: Some("whisper-1"),
        tts_model: Some("tts-1"),
        hosted: true,
    },
    // ---- servers you run yourself: default ports are overridable ----
    SpeechProvider {
        id: "litellm",
        label: "LiteLLM proxy",
        blurb: "your LiteLLM proxy — fans speech-to-text and text-to-speech out to any backend",
        base_url: "http://localhost:4000/v1",
        key_var: None,
        asr_model: Some("whisper-1"),
        tts_model: Some("tts-1"),
        hosted: false,
    },
    SpeechProvider {
        id: "voxbox",
        label: "vox-box",
        blurb: "self-hosted vox-box (GPUStack) — Whisper speech-to-text + CosyVoice voices",
        base_url: "http://localhost:8080/v1",
        key_var: None,
        asr_model: Some("faster-whisper-medium"),
        tts_model: Some("cosyvoice-300m-instruct"),
        hosted: false,
    },
    SpeechProvider {
        id: "koboldcpp",
        label: "KoboldCpp",
        blurb: "self-hosted KoboldCpp's Whisper endpoint — speech-to-text only",
        base_url: "http://localhost:5001/v1",
        key_var: None,
        asr_model: Some("whisper-1"),
        tts_model: None,
        hosted: false,
    },
    SpeechProvider {
        id: "chatterbox",
        label: "Chatterbox TTS",
        blurb: "self-hosted Chatterbox — voice-cloning text-to-speech only",
        base_url: "http://localhost:4123/v1",
        key_var: None,
        asr_model: None,
        tts_model: Some("chatterbox"),
        hosted: false,
    },
    SpeechProvider {
        id: "alltalk",
        label: "AllTalk TTS",
        blurb: "self-hosted AllTalk — XTTS/Piper voices behind the OpenAI speech API",
        base_url: "http://localhost:7851/v1",
        key_var: None,
        asr_model: None,
        tts_model: Some("tts-1"),
        hosted: false,
    },
    SpeechProvider {
        id: "local",
        label: "Local server",
        blurb: "Qwen3-ASR/TTS-1.7B via a vLLM server you run — vLLM downloads the weights (advanced)",
        base_url: "http://localhost:8000/v1",
        key_var: None,
        asr_model: Some("qwen3-asr-1.7b"),
        tts_model: Some("qwen3-tts-1.7b"),
        hosted: false,
    },
    SpeechProvider {
        id: "speaches",
        label: "Speaches",
        blurb: "self-hosted faster-whisper + Kokoro in one OpenAI-compatible server",
        base_url: "http://localhost:8000/v1",
        key_var: None,
        asr_model: Some("Systran/faster-whisper-large-v3"),
        tts_model: Some("speaches-ai/Kokoro-82M-v1.0-ONNX"),
        hosted: false,
    },
    SpeechProvider {
        id: "localai",
        label: "LocalAI",
        blurb: "self-hosted LocalAI — whisper.cpp speech-to-text + Piper voices",
        base_url: "http://localhost:8080/v1",
        key_var: None,
        asr_model: Some("whisper-1"),
        tts_model: Some("tts-1"),
        hosted: false,
    },
    SpeechProvider {
        id: "whispercpp",
        label: "whisper.cpp server",
        blurb: "whisper.cpp's own server — speech-to-text only, runs on CPU",
        base_url: "http://localhost:8080/v1",
        key_var: None,
        asr_model: Some("whisper-1"),
        tts_model: None,
        hosted: false,
    },
    SpeechProvider {
        id: "kokoro",
        label: "Kokoro-FastAPI",
        blurb: "self-hosted Kokoro voices — text-to-speech only",
        base_url: "http://localhost:8880/v1",
        key_var: None,
        asr_model: None,
        tts_model: Some("kokoro"),
        hosted: false,
    },
    SpeechProvider {
        id: "openedai",
        label: "openedai-speech",
        blurb: "self-hosted Piper/XTTS voices behind the OpenAI speech API",
        base_url: "http://localhost:8000/v1",
        key_var: None,
        asr_model: None,
        tts_model: Some("tts-1"),
        hosted: false,
    },
    SpeechProvider {
        id: "edge",
        label: "openai-edge-tts",
        blurb: "self-hosted wrapper around Microsoft Edge's free voices — text-to-speech only",
        base_url: "http://localhost:5050/v1",
        key_var: None,
        asr_model: None,
        tts_model: Some("tts-1"),
        hosted: false,
    },
    SpeechProvider {
        id: "orpheus",
        label: "Orpheus-FastAPI",
        blurb: "self-hosted Orpheus voices — expressive text-to-speech only",
        base_url: "http://localhost:5005/v1",
        key_var: None,
        asr_model: None,
        tts_model: Some("orpheus"),
        hosted: false,
    },
];

/// Resolve a provider by id. `dashscope` is accepted as an alias for `qwen`
/// (the config has shipped both spellings). Case- and whitespace-insensitive,
/// matching how the registry normalizes names.
#[must_use]
pub fn find(id: &str) -> Option<&'static SpeechProvider> {
    let key = id.trim().to_lowercase();
    let key = if key == "dashscope" { "qwen" } else { &key };
    SPEECH_PROVIDERS.iter().find(|p| p.id == key)
}

/// Ids that can serve speech-to-text, in menu order.
#[must_use]
pub fn asr_ids() -> Vec<&'static str> {
    SPEECH_PROVIDERS
        .iter()
        .filter(|p| p.asr_model.is_some())
        .map(|p| p.id)
        .collect()
}

/// Ids that can serve text-to-speech, in menu order.
#[must_use]
pub fn tts_ids() -> Vec<&'static str> {
    SPEECH_PROVIDERS
        .iter()
        .filter(|p| p.tts_model.is_some())
        .map(|p| p.id)
        .collect()
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
