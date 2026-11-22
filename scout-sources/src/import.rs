//! Runs the Lua exporters and parses their CSV output:
//! Scout's own `tools/export.lua`, and the embedded one-off export of Auctionator / Forever Guide.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use scout_core::drops::{DropName, DropNpc, DropSource, DropsExport, ObservedLoot};
use scout_core::{ItemName, Row, VendorPrice};
use serde::de::DeserializeOwned;

/// Exports past prices from Auctionator and Forever Guide SavedVariables.
const LEGACY_EXPORT: &str = include_str!("../lua/legacy_export.lua");
/// Exports the items merchants sell (Scout's prices seen in game, Forever Guide's list).
const VENDOR_EXPORT: &str = include_str!("../lua/vendor_export.lua");
/// Exports where items come from, according to Forever Guide.
const DROPS_EXPORT: &str = include_str!("../lua/drops_export.lua");

/// Bumped when `drops_export.lua` changes, so the drops are imported again.
pub const DROPS_FORMAT: u32 = 1;

/// Scout's history: `lua <AddOns/Scout/tools/export.lua> <Scout.lua>`.
pub fn run_scout_export(script: &Path, saved_variables: &Path) -> Result<Vec<Row>> {
    run_lua(Command::new("lua").arg(script).arg(saved_variables), None)
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

/// The loot Scout recorded in game: `lua export.lua <Scout.lua> --loot`.
pub fn run_scout_loot(script: &Path, saved_variables: &Path) -> Result<Vec<ObservedLoot>> {
    run_lua(
        Command::new("lua")
            .arg(script)
            .arg(saved_variables)
            .arg("--loot"),
        None,
    )
}

/// Forever Guide's item sources, the NPCs they name and the names of objects and quests:
/// `lua - <ForeverGuide> <mode>` with the embedded script, once per mode.
pub fn run_drops_export(guide: &Path) -> Result<DropsExport> {
    let run = |mode: &str| -> Command {
        let mut command = Command::new("lua");
        command.arg("-").arg(guide).arg(mode);
        command
    };
    let sources: Vec<DropSource> = run_lua(&mut run("sources"), Some(DROPS_EXPORT))?;
    let npcs: Vec<DropNpc> = run_lua(&mut run("npcs"), Some(DROPS_EXPORT))?;
    let names: Vec<DropName> = run_lua(&mut run("names"), Some(DROPS_EXPORT))?;
    Ok(DropsExport {
        sources,
        npcs,
        names,
    })
}

/// Auctionator and Forever Guide: the embedded script is fed to `lua -` on stdin.
pub fn run_legacy_export(saved_variables_dir: &Path) -> Result<Vec<Row>> {
    run_lua(
        Command::new("lua").arg("-").arg(saved_variables_dir),
        Some(LEGACY_EXPORT),
    )
}

/// `lua - <Scout.lua> [<ForeverGuide/VendorPrices.lua>]` with the embedded script.
pub fn run_vendor_export(saved_variables: &Path, guide: Option<&Path>) -> Result<Vec<VendorPrice>> {
    let mut command = Command::new("lua");
    command.arg("-").arg(saved_variables);
    if let Some(guide) = guide {
        command.arg(guide);
    }
    run_lua(&mut command, Some(VENDOR_EXPORT))
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
    use scout_core::SCOUT;

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
    fn parses_drop_and_loot_rows() {
        let csv = b"kind,item_id,source_id,chance,quest_only,more\n\
            npc,2589,3,12,0,0\nnpc,3858,0,,0,614\n";
        let rows: Vec<DropSource> = parse(csv).unwrap();
        assert_eq!(rows[0].chance, Some(12.0));
        assert_eq!((rows[1].chance, rows[1].more), (None, 614));
        let csv = b"npc_id,name,zone,kills,skins,kind,item_id,drops,quantity\n\
            3,D\xc3\xa9voreur,Bois,2,1,loot,2589,2,5\n6,,,4,0,,,,\n";
        let rows: Vec<ObservedLoot> = parse(csv).unwrap();
        assert_eq!(rows[0].name.as_deref(), Some("D\u{e9}voreur"));
        assert_eq!(
            (rows[1].kind.as_deref(), rows[1].item_id, rows[1].kills),
            (None, None, 4)
        );
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
