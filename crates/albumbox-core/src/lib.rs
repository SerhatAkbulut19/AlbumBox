//! AlbumBox domain and data access.

pub mod access;
pub mod auth;
pub mod catalog;
pub mod categories;
pub mod client;
pub mod db;
pub mod error;
pub mod faces;
pub mod jobs;
pub mod libraries;
pub mod media;
pub mod profiles;
pub mod search;
pub mod stats;
pub mod tags;
pub mod text;
pub mod users;

pub use db::Db;
pub use error::{CoreError, CoreResult};
