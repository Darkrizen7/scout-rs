//! WoW Forever's game data: the client tables from wago.tools, the items they lack from Wowhead,
//! assembled into recipes, items and maps, and the versioned package `scout-server` builds once and
//! every app downloads (`docs/serveur.md`).

mod fetch;
pub mod package;
mod patterns;
mod wago;
pub mod wowhead;

pub use fetch::{Download, Naming, download, name_items, wanted_items};
pub use package::{Manifest, Package};
pub use patterns::{
    Summary, apply_pattern_skills, merge_pattern_skills, parse_pattern_skills, write_pattern_skills,
};
pub use wago::{TABLES, assemble, latest_build, table_csv};
