pub(crate) mod activity;
pub mod cast;
pub mod db;
pub mod download;
pub mod error;
pub mod filter;
pub mod identify;
mod identify_show;
pub mod lang;
pub mod library;
pub mod matcher;
pub mod model;
pub(crate) mod mpv_ipc;
pub mod nfo;
pub mod parse;
pub mod player;
pub mod probe;
pub mod query;
mod rows;
pub mod scan;
pub mod secrets;
pub mod tmdb;
pub mod tools;
pub mod translate;
pub mod volume;

pub use error::{Error, Result};
pub use library::{
    Library, LibraryStatus, RefreshEvent, RefreshOptions, RefreshPhase, RefreshReport,
};
