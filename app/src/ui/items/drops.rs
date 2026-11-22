//! Drops tab of the item sheet: where the item comes from, Forever Guide's chance next to the user's own
//! (kills recorded by Scout).

use std::collections::HashSet;

use eframe::egui::{self, RichText};

use crate::ui::theme::Palette;
use crate::ui::widgets;
use scout_core::drops::{Drop, DropColumn, DropKind, ItemDrops, compare};

/// Rows shown per section before "Tout afficher".
const SHORT_LIST: usize = 30;

const KINDS: [DropKind; 7] = [
    DropKind::Npc,
    DropKind::Skinning,
    DropKind::Pickpocket,
    DropKind::Object,
    DropKind::Container,
    DropKind::Quest,
    DropKind::QuestChoice,
];

/// What the user chose in the tab: sections shown in full, sort column and direction (shared by all sections).
pub struct DropsView {
    expanded: HashSet<DropKind>,
    column: DropColumn,
    descending: bool,
}

impl Default for DropsView {
    fn default() -> Self {
        Self {
            expanded: HashSet::new(),
            column: DropColumn::Chance,
            descending: true,
        }
    }
}

/// Number of sources, for the tab title.
pub fn count(drops: &ItemDrops) -> i64 {
    drops.drops.len() as i64 + drops.more.iter().map(|(_, n)| n).sum::<i64>()
}

/// Shows the sources; returns the container item clicked, if any.
pub fn show(
    ui: &mut egui::Ui,
    drops: &ItemDrops,
    view: &mut DropsView,
    palette: &Palette,
) -> Option<i64> {
    let mut link = None;
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| {
            let real = match drops.total_kills {
                0 => "aucun kill enregistré par Scout pour l'instant".to_owned(),
                n => format!("{n} corps fouillés enregistrés par Scout"),
            };
            ui.weak(format!("Base : Forever Guide · Réel : {real}"));
            ui.add_space(6.0);
            if drops.drops.is_empty() && drops.more.is_empty() {
                ui.add_space(14.0);
                ui.vertical_centered(|ui| {
                    ui.weak("Aucune source connue (Forever Guide n'est pas installé ou ne connaît pas cet objet).");
                });
                return;
            }
            for kind in KINDS {
                section(ui, drops, kind, view, palette, &mut link);
            }
        });
    link
}

fn section(
    ui: &mut egui::Ui,
    drops: &ItemDrops,
    kind: DropKind,
    view: &mut DropsView,
    palette: &Palette,
    link: &mut Option<i64>,
) {
    let mut rows: Vec<&Drop> = drops.of(kind).collect();
    rows.sort_by(|a, b| compare(a, b, view.column, view.descending));
    let more = drops
        .more
        .iter()
        .find(|(k, _)| *k == kind)
        .map_or(0, |(_, n)| *n);
    if rows.is_empty() && more == 0 {
        return;
    }
    ui.strong(format!("{} ({})", kind.title(), rows.len() as i64 + more));
    let observed = matches!(kind, DropKind::Npc | DropKind::Skinning);
    let shown = if view.expanded.contains(&kind) {
        rows.len()
    } else {
        rows.len().min(SHORT_LIST)
    };
    egui::Grid::new(("drops", kind))
        .num_columns(if observed { 5 } else { 4 })
        .spacing([14.0, 3.0])
        .striped(true)
        .show(ui, |ui| {
            view.header(ui, "Source", DropColumn::Source, palette);
            if kind.is_npc() {
                view.header(ui, "Niveau", DropColumn::Level, palette);
                view.header(ui, "Lieu", DropColumn::Place, palette);
            } else {
                ui.label("");
                ui.label("");
            }
            view.header(ui, "Chance", DropColumn::Chance, palette);
            if observed {
                view.header(ui, "Réel", DropColumn::Observed, palette);
            }
            ui.end_row();
            for drop in &rows[..shown] {
                row(ui, drop, kind, observed, palette, link);
                ui.end_row();
            }
        });
    if shown < rows.len()
        && ui
            .button(format!("Tout afficher ({})", rows.len()))
            .clicked()
    {
        view.expanded.insert(kind);
    }
    if more > 0 {
        ui.weak(format!(
            "… et {more} autres sources que Forever Guide ne liste pas."
        ));
    }
    ui.add_space(10.0);
}

fn row(
    ui: &mut egui::Ui,
    drop: &Drop,
    kind: DropKind,
    observed: bool,
    palette: &Palette,
    link: &mut Option<i64>,
) {
    ui.horizontal(|ui| {
        if kind == DropKind::Container {
            if ui
                .link(&drop.name)
                .on_hover_text("Ouvrir sa fiche")
                .clicked()
            {
                *link = Some(drop.source_id);
            }
        } else {
            ui.label(&drop.name);
        }
        if drop.quest_only {
            ui.weak("· quête en cours");
        }
    });
    match &drop.npc {
        Some(npc) => {
            ui.weak(npc.levels());
            ui.weak(npc.place());
        }
        None => {
            ui.label("");
            ui.label("");
        }
    }
    widgets::chance_label(ui, drop.chance);
    if observed {
        widgets::observed_label(ui, drop.observed, palette);
    }
}

impl DropsView {
    /// A column title: click to sort by it, again to reverse.
    fn header(&mut self, ui: &mut egui::Ui, label: &str, column: DropColumn, palette: &Palette) {
        let arrow = match (self.column == column, self.descending) {
            (false, _) => "",
            (true, false) => " ⏶",
            (true, true) => " ⏷",
        };
        let text = RichText::new(format!("{label}{arrow}")).color(palette.dim_text);
        if ui
            .add(egui::Button::new(text).frame(false))
            .on_hover_text("Trier par cette colonne")
            .clicked()
        {
            self.descending = if self.column == column {
                !self.descending
            } else {
                column.descending_first()
            };
            self.column = column;
        }
    }
}
