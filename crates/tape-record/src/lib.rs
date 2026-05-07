//! Recording subsystem for `tape record`. See SPEC §8 and the
//! `tape-record-pipeline` skill.
//!
//! Public surface:
//! - [`session::Session`] — owns the in-flight recording; events are appended
//!   monotonically.
//! - [`proxy::anthropic`] — HTTP proxy that records `model_call` events while
//!   tee'ing streaming responses through to the child without buffering.
//! - [`eject::eject`] — finalizes a session into a `.tape` zip on disk.

pub mod eject;
pub mod overlay;
pub mod proxy;
pub mod run;
pub mod session;
pub mod socket;
pub mod transcript;
