//! Core of the `lolt` League of Legends esports tracker.
//!
//! The crate builds two ways:
//!
//! * as an `rlib` for the `lolt` CLI binary, and
//! * as a `staticlib` exposing the C ABI in [`ffi`], which the macOS widget
//!   links against so both front-ends share one API client and view model.

pub mod api;
pub mod commands;
pub mod ffi;
pub mod models;
pub mod render;
pub mod resolve;
pub mod snapshot;
