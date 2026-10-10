//! Forge Desktop's engine. Nothing here knows about the window: the app shell
//! (`src-tauri`) turns these into commands and one event channel.

pub mod catalog;
pub mod cc_settings;
pub mod cli;
pub mod events;
pub mod gateway;
pub mod git;
pub mod log;
pub mod meta;
pub mod recent;
pub mod session_manager;
pub mod sessions;
pub mod settings;
pub mod watch;
pub use reqwest;
