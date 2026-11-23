//! Runs the Lua exporters (embedded Lua 5.4) and parses their CSV output:
//! Scout's own `tools/export.lua`, and the embedded one-off export of Auctionator / Forever Guide.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use mlua::{Function, Lua, LuaString};
use scout_core::drops::{DropsExport, ObservedLoot};
use scout_core::{ItemName, Row, VendorPrice};
use serde::de::DeserializeOwned;

/// Exports past prices from Auctionator and Forever Guide SavedVariables.
const LEGACY_EXPORT: &str = include_str!("../lua/legacy_export.lua");
/// Exports the items merchants sell (Scout's prices seen in game, Forever Guide's list).
const VENDOR_EXPORT: &str = include_str!("../lua/vendor_export.lua");
/// Exports where items come from, according to Forever Guide.
const DROPS_EXPORT: &str = include_str!("../lua/drops_export.lua");
/// Runs an exporter in-process: stdout collected, stderr dropped, files read by Rust.
const SANDBOX: &str = include_str!("../lua/sandbox.lua");

/// Bumped when `drops_export.lua` changes, so the drops are imported again.
pub const DROPS_FORMAT: u32 = 2;

/// Scout's history: `export.lua <Scout.lua>`.
pub fn run_scout_export(script: &Path, saved_variables: &Path) -> Result<Vec<Row>> {
    run_lua(&read_script(script)?, script, &[saved_variables.into()])
}

/// Every item name Scout knows: `export.lua <Scout.lua> --names`.
pub fn run_scout_names(script: &Path, saved_variables: &Path) -> Result<Vec<ItemName>> {
    run_lua(
        &read_script(script)?,
        script,
        &[saved_variables.into(), "--names".into()],
    )
}

/// The loot Scout recorded in game: `export.lua <Scout.lua> --loot`.
pub fn run_scout_loot(script: &Path, saved_variables: &Path) -> Result<Vec<ObservedLoot>> {
    run_lua(
        &read_script(script)?,
        script,
        &[saved_variables.into(), "--loot".into()],
    )
}

/// Forever Guide's item sources, the NPCs they name and the names of objects and quests:
/// the embedded script `<ForeverGuide> <mode>`, once per mode.
pub fn run_drops_export(guide: &Path) -> Result<DropsExport> {
    let name = Path::new("drops_export.lua");
    let run = |mode: &str| [guide.into(), mode.into()];
    Ok(DropsExport {
        sources: run_lua(DROPS_EXPORT, name, &run("sources"))?,
        npcs: run_lua(DROPS_EXPORT, name, &run("npcs"))?,
        names: run_lua(DROPS_EXPORT, name, &run("names"))?,
    })
}

/// Auctionator and Forever Guide: the embedded script `<SavedVariables dir>`.
pub fn run_legacy_export(saved_variables_dir: &Path) -> Result<Vec<Row>> {
    let name = Path::new("legacy_export.lua");
    run_lua(LEGACY_EXPORT, name, &[saved_variables_dir.into()])
}

/// The embedded script `<Scout.lua> [<ForeverGuide/VendorPrices.lua>]`.
pub fn run_vendor_export(saved_variables: &Path, guide: Option<&Path>) -> Result<Vec<VendorPrice>> {
    let mut args = vec![saved_variables.into()];
    args.extend(guide.map(Arg::from));
    run_lua(VENDOR_EXPORT, Path::new("vendor_export.lua"), &args)
}

/// A command-line argument handed to an exporter (`arg[i]` and `...`).
struct Arg(String);

impl From<&Path> for Arg {
    fn from(path: &Path) -> Self {
        Self(path.to_string_lossy().into_owned())
    }
}

impl From<&str> for Arg {
    fn from(text: &str) -> Self {
        Self(text.to_owned())
    }
}

fn read_script(script: &Path) -> Result<String> {
    fs::read_to_string(script).with_context(|| format!("lecture de {}", script.display()))
}

/// Runs an exporter with the embedded Lua (no `lua` to install) and parses the CSV it writes.
fn run_lua<T: DeserializeOwned>(script: &str, name: &Path, args: &[Arg]) -> Result<Vec<T>> {
    let failed = |e: mlua::Error| anyhow!("l'export Lua a échoué : {e}");
    let lua = Lua::new();
    let read_file = lua
        .create_function(|lua, path: String| match fs::read(&path) {
            Ok(bytes) => Ok((Some(lua.create_string(bytes)?), None)),
            Err(e) => Ok((None, Some(format!("{path}: {e}")))),
        })
        .map_err(failed)?;
    let run: Function = lua
        .load(SANDBOX)
        .set_name("=sandbox")
        .call(read_file)
        .map_err(failed)?;
    let chunk = lua
        .load(script)
        .set_name(format!("@{}", name.display()))
        .into_function()
        .map_err(failed)?;
    let args: Vec<&str> = args.iter().map(|a| a.0.as_str()).collect();
    let csv: LuaString = run.call((chunk, args)).map_err(failed)?;
    parse(&csv.as_bytes())
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
    use scout_core::drops::DropSource;

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
