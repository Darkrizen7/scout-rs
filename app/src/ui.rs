//! UI panels. They never touch the disk: everything goes through the worker.

#[cfg(feature = "analysis")]
pub mod analysis_view;
pub mod calculator;
pub mod chart;
pub mod coins;
pub mod fonts;
pub mod header;
pub mod item_list;
pub mod items;
pub mod loot;
pub mod recipes;
pub mod settings;
pub mod sidebar;
pub mod theme;
pub mod widgets;
