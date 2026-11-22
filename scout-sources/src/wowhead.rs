//! Wowhead (Classic database, French): items Forever's server sends as hotfixes (mostly taken from
//! Season of Discovery), absent from the client tables wago.tools reads.

use std::time::Duration;

use anyhow::{Context, Result, bail};

use scout_core::gamedata::GameItem;

/// A single item's XML is a few kilobytes.
const MAX_BYTES: u64 = 1024 * 1024;
/// Pause between two requests: Wowhead's CDN blocks the address (403) after a few hundred quick ones.
pub const PAUSE: Duration = Duration::from_secs(1);

/// The item as Wowhead describes it, or None when Wowhead does not know it.
pub fn item(item_id: i64) -> Result<Option<GameItem>> {
    let url = format!("https://www.wowhead.com/classic/fr/item={item_id}&xml");
    let mut response = match super::wago::agent().get(&url).call() {
        Err(ureq::Error::StatusCode(403 | 429)) => bail!(
            "Wowhead limite les requêtes (trop rapprochées) : réessaie « Mettre à jour » plus tard, \
             les objets déjà lus sont gardés"
        ),
        result => result.with_context(|| format!("Wowhead, objet {item_id}"))?,
    };
    let xml = response
        .body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_string()
        .with_context(|| format!("Wowhead, objet {item_id}"))?;
    Ok(parse(item_id, &xml))
}

fn parse(item_id: i64, xml: &str) -> Option<GameItem> {
    let name = between(xml, "<name><![CDATA[", "]]></name>")?.trim();
    if name.is_empty() {
        return None;
    }
    let number = |text: Option<&str>| text.and_then(|t| t.trim().parse::<i64>().ok());
    let attribute = |tag: &str| number(between(xml, &format!("<{tag} id=\""), "\""));
    let json = |key: &str| {
        let start = xml.find(&format!("\"{key}\":"))? + key.len() + 3;
        let digits = xml[start..]
            .find(|c: char| !c.is_ascii_digit() && c != '-')
            .map_or(&xml[start..], |end| &xml[start..start + end]);
        digits.parse::<i64>().ok()
    };
    Some(GameItem {
        id: item_id,
        name: name.to_owned(),
        quality: attribute("quality").unwrap_or(1),
        item_level: number(between(xml, "<level>", "</level>")).unwrap_or(0),
        required_level: json("reqlevel").unwrap_or(0),
        class_id: attribute("class"),
        subclass_id: attribute("subclass"),
        buy_price: 0,
        vendor_stack: 1,
        sell_price: json("sellprice").unwrap_or(0),
    })
}

fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = text.find(start)? + start.len();
    let len = text[from..].find(end)?;
    Some(&text[from..from + len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_item() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><wowhead><item id="233813"><name><![CDATA[Armure en cuir de Roncerasoir]]></name><level>79</level><quality id="4">Épique</quality><class id="4"><![CDATA[Armure]]></class><subclass id="2"><![CDATA[Armures en cuir]]></subclass><json><![CDATA["id":233813,"reqlevel":60]]></json><jsonEquip><![CDATA["reqlevel":60,"sellprice":87048,"sta":38]]></jsonEquip></item></wowhead>"#;
        let item = parse(233813, xml).unwrap();
        assert_eq!(item.name, "Armure en cuir de Roncerasoir");
        assert_eq!((item.quality, item.item_level), (4, 79));
        assert_eq!((item.class_id, item.subclass_id), (Some(4), Some(2)));
        assert_eq!((item.required_level, item.sell_price), (60, 87048));
    }

    #[test]
    fn unknown_item() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?><wowhead><error>Item not found!</error></wowhead>"#;
        assert!(parse(999999, xml).is_none());
    }
}
