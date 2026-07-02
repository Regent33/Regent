use crate::multi_client::McpServerConfig;

/// Community-maintained presets for commonly used MCP server setups.
///
/// These live in `or-mcp` so callers can start from a typed config and then
/// override paths, tokens, or URLs for their local environment.
pub mod known {
    use super::McpServerConfig;

    /// Filesystem access via the reference MCP filesystem server.
    #[must_use]
    pub fn filesystem() -> McpServerConfig {
        McpServerConfig::stdio(
            "filesystem",
            "stdio://filesystem",
            "npx",
            vec![
                "-y".to_owned(),
                "@modelcontextprotocol/server-filesystem".to_owned(),
            ],
        )
    }

    /// Brave Search using the Brave-maintained MCP server.
    #[must_use]
    pub fn brave_search() -> McpServerConfig {
        McpServerConfig::stdio(
            "brave-search",
            "stdio://brave-search",
            "npx",
            vec![
                "-y".to_owned(),
                "@brave/brave-search-mcp-server".to_owned(),
                "--transport".to_owned(),
                "stdio".to_owned(),
            ],
        )
    }

    /// GitHub access via the reference MCP GitHub server.
    #[must_use]
    pub fn github() -> McpServerConfig {
        McpServerConfig::stdio(
            "github",
            "stdio://github",
            "npx",
            vec![
                "-y".to_owned(),
                "@modelcontextprotocol/server-github".to_owned(),
            ],
        )
    }

    /// Slack access using the maintained Slack MCP server.
    #[must_use]
    pub fn slack() -> McpServerConfig {
        McpServerConfig::stdio(
            "slack",
            "stdio://slack",
            "npx",
            vec!["-y".to_owned(), "@zencoderai/slack-mcp-server".to_owned()],
        )
    }

    /// PostgreSQL access via the reference MCP PostgreSQL server.
    #[must_use]
    pub fn postgres() -> McpServerConfig {
        McpServerConfig::stdio(
            "postgres",
            "stdio://postgres",
            "npx",
            vec![
                "-y".to_owned(),
                "@modelcontextprotocol/server-postgres".to_owned(),
            ],
        )
    }
}
