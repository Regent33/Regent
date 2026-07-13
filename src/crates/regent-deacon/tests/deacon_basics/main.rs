//! Integration tests for the regent-deacon (core process) layer.
//! Cover: RPC type serialisation, session lifecycle (create → list → resume),
//! turn execution with a scripted provider. One test binary; the cases live in
//! per-surface modules.

mod code_skill;
mod dispatcher_admin;
mod dispatcher_basic;
mod dispatcher_models;
mod distiller;
mod explore;
mod helpers;
mod ledger;
mod routing;
mod rpc_types;
mod sessions;
mod tiering;
mod turns;
