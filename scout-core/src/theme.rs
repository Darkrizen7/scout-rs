//! The look the user picked (the palettes themselves belong to the UI).

use serde::{Deserialize, Serialize};

/// GNOME (libadwaita) colors, like the Files app.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    #[serde(alias = "parchment")]
    Light,
    #[serde(alias = "slate")]
    Dark,
}
