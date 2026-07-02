use or_mcp::{McpServerTransport, known_servers::known};

#[test]
fn known_servers_filesystem_config_valid() {
    let config = known::filesystem();
    assert!(!config.name.is_empty());
    assert!(!config.url.is_empty());
    assert!(matches!(
        config.transport,
        McpServerTransport::Stdio { command, .. } if !command.is_empty()
    ));
}
