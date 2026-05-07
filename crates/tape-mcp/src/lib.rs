//! The deck — `tape mcp`. MCP server over stdio. See SPEC + the
//! `tape-mcp-deck` skill for the tool contract.
//!
//! This crate provides a [`Deck`] handle (the in-process state) plus a
//! [`stdio_loop`] that reads newline-delimited JSON-RPC 2.0 from stdin and
//! writes responses on stdout. The CLI binary `tape` wires `stdio_loop`
//! into the `mcp` subcommand.

pub mod deck;
pub mod tools;
pub mod jsonrpc;
pub mod server;

pub use deck::Deck;
pub use server::stdio_loop;
