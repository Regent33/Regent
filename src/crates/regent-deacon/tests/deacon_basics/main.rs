//! Integration tests for the regent-deacon (core process) layer.
//! Cover: RPC type serialisation, session lifecycle (create → list → resume),
//! turn execution with a scripted provider. One test binary; the cases live in
//! per-surface modules.

mod dispatcher_admin;
mod dispatcher_basic;
mod dispatcher_models;
mod helpers;
mod routing;
mod rpc_types;
mod sessions;
mod turns;
