pub mod data;
pub mod plain;

#[cfg(feature = "tui")]
pub mod tui;

#[cfg(feature = "tui")]
pub mod keymap;

#[cfg(feature = "tui")]
pub mod views;

pub use data::{build_show_data, LineAnnotationMap, RegionRef, ShowData};
pub use plain::run_plain;

#[cfg(feature = "tui")]
pub use tui::run_tui;
