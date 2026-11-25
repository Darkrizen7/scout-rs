//! Items an older version of the tool fetched from Wowhead (absent from the client tables). The
//! price server asks Wowhead now and puts them in its package; the table stays because the embedded
//! reference database still carries it.

pub(crate) const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS wowhead_items (
    item_id        INTEGER PRIMARY KEY,
    name           TEXT NOT NULL,
    quality        INTEGER NOT NULL,
    item_level     INTEGER NOT NULL,
    required_level INTEGER NOT NULL,
    class_id       INTEGER,
    subclass_id    INTEGER,
    buy_price      INTEGER NOT NULL DEFAULT 0,
    vendor_stack   INTEGER NOT NULL DEFAULT 1,
    sell_price     INTEGER NOT NULL DEFAULT 0
);
";
