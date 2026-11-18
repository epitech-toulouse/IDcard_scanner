//! Cœur métier de Badgeuse : base SQLite, logique d'émargement, import/export, lecteurs NFC.
//! Indépendant de Tauri pour pouvoir être testé avec un simple `cargo test`.

pub mod attendance;
pub mod db;
pub mod domain;
pub mod error;
pub mod export;
pub mod groups;
pub mod import;
pub mod model;
pub mod nfc;
pub mod remote;
pub mod sessions;
pub mod settings;
pub mod students;

pub use error::{Error, Result};
