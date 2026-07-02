use crate::application::client::NexusClient;
use crate::domain::contracts::NexusClientTrait;
use crate::domain::entities::McpTool;
use crate::domain::errors::McpError;
use crate::infra::http_transport::StreamableHttpTransport;
use crate::infra::stdio_transport::StdioTransport;
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Transport settings for a known MCP server connection.
///
/// This lives in `or-mcp`. The crate already exposes a `McpTransport` trait for
/// runtime I/O, so the configuration enum uses a distinct name to avoid clashing
/// with the existing transport abstraction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum McpServerTransport {
    /// Connect through streamable HTTP using the config `url`.
    Http,
    /// Spawn a stdio process. For stdio transports, `url` is treated as a stable
    /// logical identifier rather than a network endpoint.
    Stdio { command: String, args: Vec<String> },
}

/// Connection settings for a single MCP server endpoint managed by `MultiMcpClient`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServerConfig {
    /// Human-readable server name used for deduplication prefixes and tracing.
    pub name: String,
    /// HTTP endpoint or logical stdio identifier for this server.
    pub url: String,
    /// Transport configuration for reaching the server.
    pub transport: McpServerTransport,
    /// Optional bearer token used when `transport` is `Http`.
    pub auth: Option<String>,
}

impl McpServerConfig {
    /// Builds an HTTP MCP server config.
    #[must_use]
    pub fn http(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            transport: McpServerTransport::Http,
            auth: None,
        }
    }

    /// Builds a stdio MCP server config.
    ///
    /// For stdio transports, `url` is used as a logical identifier because there
    /// is no network endpoint to store.
    #[must_use]
    pub fn stdio(
        name: impl Into<String>,
        url: impl Into<String>,
        command: impl Into<String>,
        args: Vec<String>,
    ) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            transport: McpServerTransport::Stdio {
                command: command.into(),
                args,
            },
            auth: None,
        }
    }

    /// Adds a bearer token to an HTTP config.
    #[must_use]
    pub fn with_auth(mut self, token: impl Into<String>) -> Self {
        self.auth = Some(token.into());
        self
    }
}

/// A resolved MCP tool plus the server metadata needed to invoke it later.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiscoveredMcpTool {
    /// Name of the MCP server that exported this tool.
    pub server_name: String,
    /// Original remote tool name reported by the server.
    pub original_name: String,
    /// Locally registered tool name after collision handling.
    pub registered_name: String,
    /// Full MCP tool metadata returned by the server.
    pub tool: McpTool,
}

#[derive(Clone)]
enum ConnectedClient {
    Http(NexusClient<StreamableHttpTransport>),
    Stdio(NexusClient<StdioTransport>),
}

impl ConnectedClient {
    async fn list_tools(&self) -> Result<Vec<McpTool>, McpError> {
        match self {
            Self::Http(client) => client.list_tools().await,
            Self::Stdio(client) => client.list_tools().await,
        }
    }

    async fn invoke_tool(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        match self {
            Self::Http(client) => client.invoke_tool(name, args).await,
            Self::Stdio(client) => client.invoke_tool(name, args).await,
        }
    }
}

#[derive(Clone)]
struct ConnectedServer {
    config: McpServerConfig,
    client: ConnectedClient,
    tools: Vec<McpTool>,
}

#[derive(Clone)]
struct MultiMcpSessionInner {
    servers: Vec<ConnectedServer>,
    tools: Vec<DiscoveredMcpTool>,
    routes: HashMap<String, (usize, String)>,
}

/// A connected, merged MCP server session produced by `MultiMcpClient`.
#[derive(Clone)]
pub struct MultiMcpSession {
    inner: Arc<MultiMcpSessionInner>,
}

impl MultiMcpSession {
    /// Returns the merged tool list after duplicate names have been prefixed by server name.
    #[must_use]
    pub fn tools(&self) -> &[DiscoveredMcpTool] {
        &self.inner.tools
    }

    /// Invokes a merged tool by its registered name.
    pub async fn invoke(
        &self,
        registered_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let (server_index, original_name) = self
            .inner
            .routes
            .get(registered_name)
            .cloned()
            .ok_or_else(|| {
                McpError::ToolExecution(format!("unknown merged tool: {registered_name}"))
            })?;
        self.inner.servers[server_index]
            .client
            .invoke_tool(&original_name, args)
            .await
    }
}

/// Connects to multiple MCP servers and merges their tools into a deduplicated namespace.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiMcpClient {
    servers: Vec<McpServerConfig>,
}

impl MultiMcpClient {
    /// Creates an empty multi-server MCP client builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds another MCP server to the merged client configuration.
    #[must_use]
    pub fn add_server(mut self, config: McpServerConfig) -> Self {
        self.servers.push(config);
        self
    }

    /// Connects to every configured server concurrently and returns a merged session.
    pub async fn connect_all(self) -> Result<MultiMcpSession, McpError> {
        let servers = try_join_all(self.servers.into_iter().map(connect_server)).await?;
        let tools = merge_tools(&servers);
        let routes = tools
            .iter()
            .map(|tool| {
                let server_index = servers
                    .iter()
                    .position(|server| server.config.name == tool.server_name)
                    .ok_or_else(|| {
                        McpError::Protocol("merged tool referenced a missing server".to_owned())
                    })?;
                Ok((
                    tool.registered_name.clone(),
                    (server_index, tool.original_name.clone()),
                ))
            })
            .collect::<Result<HashMap<_, _>, McpError>>()?;
        Ok(MultiMcpSession {
            inner: Arc::new(MultiMcpSessionInner {
                servers,
                tools,
                routes,
            }),
        })
    }
}

async fn connect_server(config: McpServerConfig) -> Result<ConnectedServer, McpError> {
    let client = match &config.transport {
        McpServerTransport::Http => ConnectedClient::Http(NexusClient::new(
            StreamableHttpTransport::with_bearer_token(config.url.clone(), config.auth.clone()),
        )),
        McpServerTransport::Stdio { command, args } => {
            let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
            ConnectedClient::Stdio(NexusClient::connect_stdio(command, &refs)?)
        }
    };
    let tools = client.list_tools().await?;
    Ok(ConnectedServer {
        config,
        client,
        tools,
    })
}

fn merge_tools(servers: &[ConnectedServer]) -> Vec<DiscoveredMcpTool> {
    let mut counts = HashMap::<String, usize>::new();

    for server in servers {
        for tool in &server.tools {
            *counts.entry(tool.name.clone()).or_default() += 1;
        }
    }

    servers
        .iter()
        .flat_map(|server| {
            let counts = counts.clone();
            server.tools.iter().cloned().map(move |tool| {
                let server_name = server.config.name.clone();
                let registered_name = if counts.get(&tool.name).copied().unwrap_or_default() > 1 {
                    format!("{server_name}::{}", tool.name)
                } else {
                    tool.name.clone()
                };
                DiscoveredMcpTool {
                    server_name: server_name.clone(),
                    original_name: tool.name.clone(),
                    registered_name,
                    tool,
                }
            })
        })
        .collect()
}
