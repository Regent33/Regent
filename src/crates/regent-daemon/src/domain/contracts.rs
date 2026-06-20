use regent_providers::ChatProvider;
use std::sync::Arc;

/// Outbound write handle — cheaply cloneable and Send. Shared between the
/// main dispatcher and per-session tasks that stream notifications.
pub type OutboundTx = tokio::sync::mpsc::UnboundedSender<String>;

/// Builds a provider for a given model id. Lets the daemon switch models at
/// runtime (`model.set`) by constructing a fresh provider per session — model
/// changes apply to new sessions only, preserving each session's prompt cache.
pub type ProviderFactory = Arc<dyn Fn(&str) -> Arc<dyn ChatProvider> + Send + Sync>;
