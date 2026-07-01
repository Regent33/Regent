use regent_providers::ChatProvider;
use regent_tools::DeliverySink;
use std::sync::Arc;

/// Outbound write handle — cheaply cloneable and Send. Shared between the
/// main dispatcher and per-session tasks that stream notifications.
pub type OutboundTx = tokio::sync::mpsc::UnboundedSender<String>;

/// Resolves a keyed conversation (`"platform:chat_id"`) to a delivery sink that
/// routes the agent's `send_message`/`send_file` back to that platform's API.
/// Implemented over the webhook adapter registry; returns `None` for keys that
/// aren't a known outbound webhook target (so the session falls back to the
/// CLI-notification sink). Set on the [`SessionManager`] once the webhook
/// registry is built, so platform sessions deliver to the platform while local
/// CLI sessions keep notifying the connected client.
///
/// [`SessionManager`]: crate::application::session_manager::SessionManager
pub trait PlatformDelivery: Send + Sync {
    fn sink_for(&self, conversation_key: &str) -> Option<Arc<dyn DeliverySink>>;
}

/// Builds a provider for a given model id. Lets the deacon switch models at
/// runtime (`model.set`) by constructing a fresh provider per session — model
/// changes apply to new sessions only, preserving each session's prompt cache.
pub type ProviderFactory = Arc<dyn Fn(&str) -> Arc<dyn ChatProvider> + Send + Sync>;
