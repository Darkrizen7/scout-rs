//! Text search shared by every search field: each typed word must appear in the name, in any
//! order, ignoring case and accents ("armure roncerasoire" finds "Armure en cuir de roncerasoire").

/// A parsed search: normalized words, plus the raw text to match an item number.
#[derive(Debug, Clone, Default)]
pub struct Query {
    text: String,
    words: Vec<String>,
}

impl Query {
    pub fn new(text: &str) -> Self {
        let text = text.trim();
        Self {
            text: text.to_owned(),
            words: normalize(text)
                .split_whitespace()
                .map(str::to_owned)
                .collect(),
        }
    }

    /// Nothing typed: everything matches.
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Every word appears in `name` (always true for an empty query).
    pub fn matches(&self, name: &str) -> bool {
        if self.is_empty() {
            return true;
        }
        let name = normalize(name);
        self.words.iter().all(|w| name.contains(w.as_str()))
    }

    /// The query is exactly this item number.
    pub fn is_id(&self, id: i64) -> bool {
        self.text.parse() == Ok(id)
    }
}

/// Lowercase, without accents, with straight apostrophes.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            'à' | 'â' | 'ä' | 'á' | 'ã' => out.push('a'),
            'é' | 'è' | 'ê' | 'ë' => out.push('e'),
            'î' | 'ï' | 'í' | 'ì' => out.push('i'),
            'ô' | 'ö' | 'ó' | 'ò' | 'õ' => out.push('o'),
            'ù' | 'û' | 'ü' | 'ú' => out.push('u'),
            'ÿ' => out.push('y'),
            'ç' => out.push('c'),
            'ñ' => out.push('n'),
            'œ' => out.push_str("oe"),
            'æ' => out.push_str("ae"),
            '’' | '‘' | '`' => out.push('\''),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_in_any_order_with_gaps() {
        let name = "Armure en cuir de roncerasoire";
        assert!(Query::new("Armure roncerasoire").matches(name));
        assert!(Query::new("roncerasoire  armure").matches(name));
        assert!(Query::new("arm ronce").matches(name));
        assert!(!Query::new("armure fer").matches(name));
    }

    #[test]
    fn ignores_case_accents_and_apostrophes() {
        assert!(Query::new("epee").matches("Épée longue"));
        assert!(Query::new("COEUR").matches("Cœur de flamme"));
        assert!(Query::new("l'ours").matches("Griffe de l’ours"));
    }

    #[test]
    fn empty_and_id() {
        assert!(Query::new("   ").matches("n'importe quoi"));
        assert!(Query::new(" 2589 ").is_id(2589));
        assert!(!Query::new("2589 x").is_id(2589));
    }
}
