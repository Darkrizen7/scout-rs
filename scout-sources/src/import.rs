//! Runs the Lua exporters (embedded Lua 5.4) and parses their CSV output:
//! Scout's own `tools/export.lua` and the embedded merchant export.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow};
use mlua::{Function, Lua, LuaString};
use scout_core::drops::ObservedLoot;
use scout_core::{ItemName, Row, VendorPrice};
use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Exports the items merchants sell (the prices Scout saw in game).
const VENDOR_EXPORT: &str = include_str!("../lua/vendor_export.lua");
/// Exports the skill level of the recipe items Scout saw in game.
const PATTERN_EXPORT: &str = include_str!("../lua/pattern_export.lua");
/// Runs an exporter in-process: stdout collected, stderr dropped, files read by Rust.
const SANDBOX: &str = include_str!("../lua/sandbox.lua");

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

/// The embedded script `<Scout.lua>`.
pub fn run_vendor_export(saved_variables: &Path) -> Result<Vec<VendorPrice>> {
    run_lua(
        VENDOR_EXPORT,
        Path::new("vendor_export.lua"),
        &[saved_variables.into()],
    )
}

/// The skill level of a recipe item (`ScoutDB.craft.patterns[item].req`).
#[derive(Debug, Deserialize)]
pub struct PatternSkill {
    pub item_id: i64,
    pub skill: i64,
}

/// The embedded script `pattern_export.lua <Scout.lua>`.
pub fn run_pattern_export(saved_variables: &Path) -> Result<Vec<PatternSkill>> {
    run_lua(
        PATTERN_EXPORT,
        Path::new("pattern_export.lua"),
        &[saved_variables.into()],
    )
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

    use super::*;

    #[test]
    fn exports_pattern_levels() {
        let dir = std::env::temp_dir().join(format!("scout-pattern-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Scout.lua");
        fs::write(
            &file,
            "ScoutDB = { craft = { patterns = {\n\
             [6044] = { name = 'a', req = 150 },\n\
             [7976] = { name = 'b' },\n\
             [7977] = { name = 'c', req = 'x' },\n\
             ['oops'] = { req = 10 },\n\
             } } }",
        )
        .unwrap();
        let rows = run_pattern_export(&file).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].item_id, rows[0].skill), (6044, 150));
    }

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
    fn parses_realm_and_faction() {
        let csv = b"item_id,name,timestamp,datetime,price,quantity,realm,faction\n\
            728,,1795258800,2026-11-21 12:00:00,6789,24,Soulseeker,Horde\n\
            783,,1795258800,2026-11-21 12:00:00,,0,,\n";
        let rows: Vec<Row> = parse(csv).unwrap();
        assert_eq!(rows[0].realm.as_deref(), Some("Soulseeker"));
        assert_eq!(rows[0].faction.as_deref(), Some("Horde"));
        assert_eq!(
            (rows[1].realm.clone(), rows[1].faction.clone()),
            (None, None)
        );
        let old = b"item_id,name,timestamp,datetime,price,quantity\n1,,5,x,9,1\n";
        assert_eq!(parse::<Row>(old).unwrap()[0].faction, None);
    }

    #[test]
    fn parses_loot_rows() {
        let csv = b"npc_id,name,zone,kills,skins,kind,item_id,drops,quantity\n\
            3,D\xc3\xa9voreur,Bois,2,1,loot,2589,2,5\n6,,,4,0,,,,\n";
        let rows: Vec<ObservedLoot> = parse(csv).unwrap();
        assert_eq!(rows[0].name.as_deref(), Some("D\u{e9}voreur"));
        assert_eq!(
            (rows[1].kind.as_deref(), rows[1].item_id, rows[1].kills),
            (None, None, 4)
        );
    }
}
