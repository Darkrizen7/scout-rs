//! wago.tools: newest WoW Forever build and its client tables as CSV (French names).

use std::time::Duration;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// Client tables the Recipes module needs.
pub const TABLES: [&str; 11] = [
    "ItemClass",
    "ItemSubClass",
    "ItemSparse",
    "Item",
    "ItemEffect",
    "ItemXItemEffect",
    "SpellName",
    "SpellEffect",
    "SpellReagents",
    "SkillLineAbility",
    "SkillLine",
];

/// WoW Forever's product on wago.tools.
const PRODUCT: &str = "wowf";
const LOCALE: &str = "frFR";
const USER_AGENT: &str = "scout-prix (outil personnel, données de recettes)";
/// The largest table (ItemSparse) is about 6 MB.
const MAX_BYTES: u64 = 64 * 1024 * 1024;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(USER_AGENT)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .build()
        .into()
}

#[derive(Deserialize)]
struct Build {
    version: String,
}

fn version_key(version: &str) -> Vec<u64> {
    version.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// Newest Forever client build listed by wago.tools (e.g. `1.60.1.71037`).
pub fn latest_build() -> Result<String> {
    let mut response = agent()
        .get("https://wago.tools/api/builds")
        .call()
        .context("liste des versions sur wago.tools")?;
    let reader = response.body_mut().with_config().limit(MAX_BYTES).reader();
    let products: std::collections::HashMap<String, Vec<Build>> =
        serde_json::from_reader(reader).context("liste des versions illisible")?;
    let Some(builds) = products.get(PRODUCT) else {
        bail!("wago.tools ne liste plus le produit « {PRODUCT} » (WoW Forever)");
    };
    builds
        .iter()
        .map(|b| b.version.clone())
        .filter(|v| v.starts_with("1."))
        .max_by_key(|v| version_key(v))
        .context("aucune version de WoW Forever sur wago.tools")
}

/// One client table of `build`, as CSV.
pub fn table_csv(table: &str, build: &str) -> Result<Vec<u8>> {
    let url = format!("https://wago.tools/db2/{table}/csv?build={build}&locale={LOCALE}");
    let mut response = agent()
        .get(&url)
        .call()
        .with_context(|| format!("téléchargement de {table}"))?;
    response
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .with_context(|| format!("lecture de {table}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_versions_numerically() {
        assert!(version_key("1.60.1.71037") > version_key("1.60.1.9999"));
        assert!(version_key("1.61.0.1") > version_key("1.60.1.71037"));
    }
}
