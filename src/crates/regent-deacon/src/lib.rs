//! regent-deacon â€” long-lived JSON-RPC 2.0 core process (canonical `app/`
//! composition root).
//!
//! Two transport modes (ADR-011):
//! - child-process (stdio): Go CLI spawns the deacon and communicates over
//!   stdin/stdout with newline-delimited JSON-RPC 2.0.
//! - attach (named pipe / Unix socket): future surfaces connect to a running
//!   deacon without spawning a new one. (P1.1: stdio only.)
//!
//! Internal layout follows ADR-007 (domain / application / infra).

pub mod application;
pub mod domain;
pub mod infra;

pub use application::background::{
    attach_embedder, spawn_curator, spawn_pending_expiry, spawn_ttl_purge,
};
pub use application::board_dispatch::spawn_board_dispatcher;
pub use application::constitution::sync_constitution;
pub use application::dispatcher::{ConfigReload, Dispatcher};
pub use application::http_serve::spawn_http_listener;
pub use application::provider_factory::make_provider_factory;
pub use application::provider_registry::{ProviderRegistry, RegistryError};
pub use application::session_manager::{AdminDeps, SessionManager};
pub use domain::cache_policy::cache_policy_for_source;
pub use domain::config::{BoardConfig, DeaconConfig, HttpConfig, ProviderKind, ToolsConfig};
pub use domain::contracts::{OutboundTx, ProviderFactory};
pub use domain::entities::{RpcNotification, RpcRequest, RpcResponse};
pub use domain::errors::DeaconError;
pub use domain::ledger::{Bust, Ledger, Segment, Tier};
pub use infra::config_loader::{expand_tilde, load_config};
pub use infra::logging::init_logging;
pub use infra::transport::{StdioTransport, spawn_write_loop};
