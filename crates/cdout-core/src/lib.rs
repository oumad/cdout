//! cdout's backend, shared by every UI shell (the Tauri app and the native
//! egui app). Nothing in this crate may depend on a UI framework: shells talk
//! to it through plain functions ([`api`]) and the [`features::agent::ChunkSink`]
//! streaming trait.

pub mod api;
pub mod constants;
pub mod features;
pub mod llm;
pub mod platform;
pub mod tools;
pub mod utils;
