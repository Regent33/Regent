//! Concrete provider backends behind the kernel `AsrProvider`/`TtsProvider`
//! contracts. `remote` (OpenAI-compatible — OpenAI / Groq / DashScope-Qwen) is
//! the default-serving path; local (whisper.cpp / piper) and `command`
//! (shell-template) backends land alongside it.

pub mod remote;
