//! LiuZX SKF Service — library target.
//!
//! Phase 1 introduced this library so that logic can be tested independently of
//! the binary entry point (`src/main.rs`). Before this split the crate was
//! binary-only, which is why `cargo test` ran zero tests.
//!
//! Module ownership during the migration:
//!
//! | Module | Status |
//! |--------|--------|
//! | [`crypto`] | Extracted from `main.rs` (pure encoding helpers) |
//! | [`config`] | Configuration load, `%VAR%` expansion, provider path resolution |
//! | [`protocol`] | Request/response types and typed parameter access |
//! | [`provider`] | Provider abstraction over the SKF C ABI (native + fake) |
//! | [`server`] | Transport, bind/serve split, and the session seam |
//! | [`session`] | Session identity, authorization, and opaque resource ownership |
//! | [`skf`] | SKF C ABI adapter (unchanged) |
//!
//! Still owned by the binary crate and scheduled for later phases:
//! the JSON-RPC dispatcher and its 37 method branches (Phase 2, decisions
//! D-06/D-07/D-08), and the Windows SCM module (migrated in Phase 1 plan 01-04).

pub mod config;
pub mod crypto;
pub mod domain;
pub mod protocol;
pub mod provider;
pub mod server;
pub mod session;
pub mod skf;

#[cfg(windows)]
pub mod win_service;
