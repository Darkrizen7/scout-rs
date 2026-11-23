//! Everything that comes from outside: the WoW folder, Scout's Lua exports (and the other addons'),
//! wago.tools, Wowhead and the tool's own releases on GitHub.

mod addon;
pub mod github;
mod import;
mod tga;
mod wago;
pub mod wow;
pub mod wowhead;

pub use addon::{addon_install, install_addon};
pub use import::{
    DROPS_FORMAT, run_drops_export, run_legacy_export, run_scout_export, run_scout_loot,
    run_scout_names, run_vendor_export,
};
pub use tga::zone_masks;
pub use wago::{TABLES, assemble, latest_build, table_csv};
