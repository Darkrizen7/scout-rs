//! Runs the Lua exporters and parses their CSV output:
//! Scout's own `tools/export.lua`, and the embedded one-off export of Auctionator / Forever Guide.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Exports past prices from Auctionator and Forever Guide SavedVariables.
const LEGACY_EXPORT: &str = include_str!("../tools/legacy_export.lua");

pub const SCOUT: &str = "scout";

/// One CSV line: an item seen (or missing) at one auction house scan.
#[derive(Debug, Deserialize)]
pub struct Row {
    /// Addon the price comes from (`scout`, `auctionator`, `foreverguide`).
    #[serde(default = "scout_source")]
    pub source: String,
    pub item_id: i64,
    #[serde(default)]
    pub name: Option<String>,
    pub timestamp: i64,
    /// Unit buyout in copper; empty when the item was not listed.
    pub price: Option<i64>,
    /// Units listed; empty when the source did not record it.
    pub quantity: Option<i64>,
}

fn scout_source() -> String {
    SCOUT.to_owned()
}

/// Scout's history: `lua <AddOns/Scout/tools/export.lua> <Scout.lua>`.
pub fn run_scout_export(script: &Path, saved_variables: &Path) -> Result<Vec<Row>> {
    run_lua(Command::new("lua").arg(script).arg(saved_variables), None)
}

/// An item name Scout saw at a scan (any item, not only the tracked ones).
#[derive(Debug, Deserialize)]
pub struct ItemName {
    pub item_id: i64,
    pub name: String,
}

/// Every item name Scout knows: `lua export.lua <Scout.lua> --names`.
pub fn run_scout_names(script: &Path, saved_variables: &Path) -> Result<Vec<ItemName>> {
    run_lua(
        Command::new("lua")
            .arg(script)
            .arg(saved_variables)
            .arg("--names"),
        None,
    )
}

/// Auctionator and Forever Guide: the embedded script is fed to `lua -` on stdin.
pub fn run_legacy_export(saved_variables_dir: &Path) -> Result<Vec<Row>> {
    run_lua(
        Command::new("lua").arg("-").arg(saved_variables_dir),
        Some(LEGACY_EXPORT),
    )
}

fn run_lua<T: DeserializeOwned>(command: &mut Command, stdin: Option<&str>) -> Result<Vec<T>> {
    let mut child = command
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("impossible de lancer `lua` (est-il installé ?)")?;
    if let Some(script) = stdin {
        // Dropping the handle closes stdin so lua starts running the script.
        child
            .stdin
            .take()
            .context("stdin de lua")?
            .write_all(script.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        bail!(
            "l'export Lua a échoué : {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parse(&output.stdout)
}

fn parse<T: DeserializeOwned>(csv: &[u8]) -> Result<Vec<T>> {
    let mut reader = csv::Reader::from_reader(csv);
    let rows = reader
        .deserialize()
        .collect::<Result<Vec<T>, _>>()
        .context("CSV d'export invalide")?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_listed_and_missing_items() {
        let csv = b"item_id,name,timestamp,datetime,price,quantity\n\
            728,\"Patron : \"\"cape\"\", verte\",1795258800,2026-11-21 12:00:00,6789,24\n\
            783,,1795258800,2026-11-21 12:00:00,,0\n";
        let rows: Vec<Row> = parse(csv).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name.as_deref(), Some("Patron : \"cape\", verte"));
        assert_eq!(rows[0].price, Some(6789));
        assert_eq!(rows[1].name, None);
        assert_eq!(rows[1].price, None);
        assert_eq!(rows[1].quantity, Some(0));
        assert_eq!(rows[1].source, SCOUT);
    }

    #[test]
    fn parses_legacy_rows() {
        let csv = b"source,item_id,timestamp,price,quantity\n\
            auctionator,2589,1794222000,18,\n";
        let rows: Vec<Row> = parse(csv).unwrap();
        assert_eq!(rows[0].source, "auctionator");
        assert_eq!(rows[0].name, None);
        assert_eq!(rows[0].quantity, None);
    }
}
