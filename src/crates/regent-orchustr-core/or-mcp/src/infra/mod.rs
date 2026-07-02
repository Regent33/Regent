//! JSON-RPC encoding lives in `jsonrpc`.
//! Concrete transports live in `http_transport` and `stdio_transport`.

pub mod http_transport;
pub mod jsonrpc;
pub mod stdio_transport;
