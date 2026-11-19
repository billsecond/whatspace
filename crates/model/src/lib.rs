//! WhatSpace's model: the tree of folders and files a scan found, with totals that are kept up
//! to date as it grows, and everything that is worked out from it without touching the disk:
//! statistics by file type and age, the largest files, the treemap layout, how sizes, counts
//! and dates are written, and export to CSV and JSON.
//!
//! No I/O happens here except writing an export to whatever `Write` it is given, so all of it
//! is tested with trees built in memory.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod category;
pub mod export;
pub mod stats;
pub mod time;
pub mod tree;
pub mod treemap;
pub mod units;

pub use category::Category;
pub use tree::{Entry, Flags, Kind, Metric, Node, NodeId, Tree};
pub use units::{Scale, SizeFormat, UnitStyle};
