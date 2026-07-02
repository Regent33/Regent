pub mod application;
pub mod domain;
pub mod infra;
pub mod known_servers;
pub mod multi_client;

pub use application::client::NexusClient;
pub use application::orchestrators::JsonRpcOrchestrator;
pub use application::server::NexusServer;

/// Crate-name-aligned alias for [`NexusClient`]; `Nexus` is the original
/// thematic name and both spellings are supported.
pub type McpClient<T> = NexusClient<T>;

/// Crate-name-aligned alias for [`NexusServer`]; `Nexus` is the original
/// thematic name and both spellings are supported.
pub type McpServer<T> = NexusServer<T>;
pub use domain::contracts::{McpTransport, NexusClientTrait, NexusServerTrait};
pub use domain::entities::{
    JsonRpcErrorDetail, JsonRpcErrorResponse, JsonRpcId, JsonRpcMessage, JsonRpcNotification,
    JsonRpcPacket, JsonRpcRequest, JsonRpcSuccessResponse, McpPrompt, McpResource, McpTask,
    McpTool, ServerCard,
};
pub use domain::errors::McpError;
pub use infra::http_transport::StreamableHttpTransport;
pub use infra::stdio_transport::StdioTransport;
pub use multi_client::{
    DiscoveredMcpTool, McpServerConfig, McpServerTransport, MultiMcpClient, MultiMcpSession,
};
