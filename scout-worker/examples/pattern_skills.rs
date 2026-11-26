//! Updates the recipe item levels of scout-server (`scout-server/data/pattern_skills.csv`) from the
//! levels the addon saw in game (`ScoutDB.craft.patterns[*].req`):
//! `cargo run -p scout-worker --example pattern_skills [path of Scout.lua]`
//! (default: the Scout.lua of the WoW folder in the settings, only read). Then commit the file
//! and deploy the server.

use std::path::{Path, PathBuf};

use anyhow::Context;

fn main() -> anyhow::Result<()> {
    let source = match std::env::args_os().nth(1) {
        Some(path) => PathBuf::from(path),
        None => {
            let config = scout_worker::Config::load();
            scout_sources::wow::saved_variables(config.wow_dir()?)?
        }
    };
    let file =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../scout-server/data/pattern_skills.csv");
    let known = match std::fs::read_to_string(&file) {
        Ok(csv) => scout_gamedata::parse_pattern_skills(&csv),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Default::default(),
        Err(e) => return Err(e).with_context(|| format!("lecture de {}", file.display())),
    };
    let seen: Vec<(i64, i64)> = scout_sources::run_pattern_export(&source)?
        .into_iter()
        .map(|p| (p.item_id, p.skill))
        .collect();
    let (merged, summary) = scout_gamedata::merge_pattern_skills(&known, &seen);

    println!("Relevé : {} ({} patrons)", source.display(), seen.len());
    println!("Ajoutés : {}", summary.added);
    println!("Modifiés : {}", summary.changed.len());
    for (item, old, new) in &summary.changed {
        println!("  {item} : {old} → {new}");
    }
    println!("Inchangés : {}", summary.unchanged);
    if summary.added == 0 && summary.changed.is_empty() {
        println!("À jour : rien n'est écrit.");
        return Ok(());
    }
    std::fs::write(&file, scout_gamedata::write_pattern_skills(&merged))
        .with_context(|| format!("écriture de {}", file.display()))?;
    println!("Écrit : {}", file.display());
    Ok(())
}
