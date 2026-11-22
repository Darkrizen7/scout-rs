//! Sorting the sources of an item by a column of the Drops tab.

use std::cmp::Ordering;

use super::Drop;

/// A column of the Drops tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropColumn {
    Source,
    Level,
    Place,
    /// Forever Guide's chance.
    Chance,
    /// The user's rate (Scout), then the number of corpses.
    Observed,
}

impl DropColumn {
    /// Direction of a first click: highest chances and levels first, names in alphabetical order.
    pub fn descending_first(self) -> bool {
        matches!(self, Self::Level | Self::Chance | Self::Observed)
    }
}

/// Orders two sources by a column. Unknown values (no chance, never killed, no zone…) stay last whatever the
/// direction; ties by name.
pub fn compare(a: &Drop, b: &Drop, column: DropColumn, descending: bool) -> Ordering {
    let by = match column {
        DropColumn::Source => Some(direction(
            a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            descending,
        )),
        DropColumn::Level => known(level(a), level(b), descending),
        DropColumn::Place => known(place(a), place(b), descending),
        DropColumn::Chance => known(a.chance, b.chance, descending),
        DropColumn::Observed => known(observed(a), observed(b), descending),
    };
    by.unwrap_or(Ordering::Equal)
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then(a.source_id.cmp(&b.source_id))
}

fn direction(order: Ordering, descending: bool) -> Ordering {
    if descending { order.reverse() } else { order }
}

/// Known values in the asked direction, unknown ones after them.
fn known<T: PartialOrd>(x: Option<T>, y: Option<T>, descending: bool) -> Option<Ordering> {
    Some(match (x, y) {
        (Some(x), Some(y)) => direction(x.partial_cmp(&y)?, descending),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    })
}

fn level(d: &Drop) -> Option<(i64, i64)> {
    let npc = d.npc.as_ref()?;
    let min = npc.min_level.or(npc.max_level)?;
    Some((min, npc.max_level.unwrap_or(min)))
}

fn place(d: &Drop) -> Option<String> {
    let text = d.npc.as_ref()?.place().to_lowercase();
    (!text.is_empty()).then_some(text)
}

fn observed(d: &Drop) -> Option<(f64, i64)> {
    d.observed.map(|o| (o.rate(), o.total))
}
