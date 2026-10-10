//! Builds osu!stable Songs folders out of osu!lazer's content-addressed file store.
//!
//! Assets become hard links to lazer's blobs, so a set costs no extra disk space.
//! `.osu` and `.osb` files are copied, because stable rewrites them in place.
//! `relink` turns stable assets that are plain copies of blobs into such links.

mod materialize;
mod relink;

pub use materialize::*;
pub use relink::*;

pub(crate) use materialize::{is_temp_name, write_replacing};
