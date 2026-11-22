//! What the database returns about auction house prices.

/// An item with its latest scan, for the list view.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct ItemSummary {
    pub item_id: i64,
    pub name: Option<String>,
    pub class_id: Option<i64>,
    pub subclass_id: Option<i64>,
    pub class_name: Option<String>,
    pub subclass_name: Option<String>,
    pub price: Option<i64>,
    pub quantity: Option<i64>,
    pub ts: i64,
    pub scans: i64,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct Snapshot {
    pub source: String,
    pub ts: i64,
    pub price: Option<i64>,
    pub quantity: Option<i64>,
}
