//! The public `/thanks` page: the data sources of `scout_core::credits`, rendered once.

use scout_core::credits::SOURCES;
use std::sync::LazyLock;

static PAGE: LazyLock<String> = LazyLock::new(render);

/// The page, built on first use.
pub fn page() -> &'static str {
    &PAGE
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render() -> String {
    let mut list = String::new();
    for source in SOURCES {
        let name = escape(source.name);
        let title = match source.url {
            Some(url) => format!("<a href=\"{}\">{name}</a>", escape(url)),
            None => name,
        };
        list.push_str(&format!(
            "<h2>{title}</h2>\n<p>{}</p>\n",
            escape(source.what)
        ));
    }
    format!(
        r#"<!doctype html>
<html lang="fr">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Scout — remerciements</title>
<style>
  body {{ font: 16px/1.55 system-ui, sans-serif; max-width: 44rem; margin: 2rem auto; padding: 0 1rem; color: #222; }}
  h1 {{ font-size: 1.5rem; }} h2 {{ font-size: 1.1rem; margin-top: 1.8rem; }}
  @media (prefers-color-scheme: dark) {{ body {{ background: #16181c; color: #ddd; }} a {{ color: #8ab4f8; }} }}
</style>
</head>
<body>
<h1>Scout — remerciements</h1>
<p>Scout est un outil gratuit, sans publicité, pour WoW: Forever. Il ne tient que grâce au travail des
personnes et des projets qui partagent leurs données : merci à elles.</p>
{list}</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_html() {
        assert_eq!(escape("a<b>&\"c"), "a&lt;b&gt;&amp;&quot;c");
    }

    #[test]
    fn lists_every_source() {
        assert!(SOURCES.iter().all(|s| page().contains(&escape(s.name))));
    }
}
