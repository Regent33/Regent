//! regent-tools — the execution side of the two-file tool contract
//! (canonical `agents/tools`).
//!
//! Clean-architecture internal layout: `domain/` (the `ToolExecutor` and
//! approval contracts, the `ToolContext` entity, the pure dangerous-command
//! guard), `application/` (the catalog orchestrator + explicit registration
//! manifests), `infra/` (the executors: terminal, files, search, memory,
//! skills). Every dispatch returns a JSON string; errors are wrapped, never
//! thrown past the catalog boundary (a core invariant).

pub mod application;
pub mod domain;
pub mod infra;

pub use application::catalog::ToolCatalog;
pub use application::registry::{core_catalog, core_catalog_from_env};
pub use domain::contracts::{
    AllowAll, ApprovalDecision, ApprovalHandler, DenyAll, PermissionAction, PermissionRule,
    ToolExecutor, VoiceScopedApprover, evaluate_permissions, wildcard_match,
};
pub use domain::contracts::{
    CommandOutput, DeliverySink, DispatchHook, NoDelivery, TerminalBackend,
};
pub use domain::entities::ToolContext;
pub use infra::ask_user::register_ask_user_tool;
pub use infra::backends::{
    DockerBackend, LocalBackend, SshBackend, parse_backend, terminal_backend_from_env,
};
pub use infra::browser::{BROWSER_MCP_ENV, attach_browser_if_configured};
pub use infra::checkpoint::{CheckpointInfo, CheckpointStore};
pub use infra::kanban_tools::register_kanban_tool;
pub use infra::key_tool::{
    MANAGED, env_var_status, extra_key_groups, key_group, register_key_tool, remove_env_var,
    swap_env_vars, upsert_env_var,
};
pub use infra::mcp_server::{StdioServerTransport, build_server, serve_catalog, server_card};
pub use infra::mcp_tools::{register_mcp_http, register_mcp_tools};
pub use infra::memory_tools::register_memory_tools;
pub use infra::message_tools::{register_file_tool, register_message_tool};
pub use infra::persona_tool::register_persona_tool;
pub use infra::read_document::register_read_document_tool;
pub use infra::sandbox::{SandboxBackend, build_sandbox_args, is_secret_env_var, sandbox_enabled};
pub use infra::shell_hook::ShellHook;
pub use infra::skill_tools::register_skill_tools;
pub use infra::todo::register_todo_tool;
