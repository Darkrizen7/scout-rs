//! Profit calculator: buy price, sell price, quantity and auction house cut.

use eframe::egui;

use crate::app::{State, item_name};
use crate::money;
use crate::ui::theme;

pub struct Calculator {
    buy: String,
    sell: String,
    quantity: u32,
    cut_percent: f64,
}

impl Default for Calculator {
    fn default() -> Self {
        Self {
            buy: String::new(),
            sell: String::new(),
            quantity: 1,
            cut_percent: 5.0,
        }
    }
}

impl Calculator {
    pub fn show(&mut self, ui: &mut egui::Ui, state: &State) {
        ui.heading("Calculateur de rentabilité");
        ui.small("Prix unitaires : « 1g 20s 5c », « 75s » ou un nombre en or (« 1,5 » = 1g 50s).");
        ui.add_space(8.0);

        egui::Grid::new("calculator")
            .num_columns(3)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                money_field(ui, "Prix d'achat (unité)", &mut self.buy);
                ui.end_row();
                money_field(ui, "Prix de vente (unité)", &mut self.sell);
                if let Some(item) = state.selected_item()
                    && let Some(price) = item.price
                    && ui
                        .button(format!("Dernier prix HdV de {}", item_name(item)))
                        .clicked()
                {
                    self.sell = money::format(price);
                }
                ui.end_row();
                ui.label("Quantité");
                ui.add(egui::DragValue::new(&mut self.quantity).range(1..=100_000));
                ui.end_row();
                ui.label("Commission HdV");
                ui.add(
                    egui::DragValue::new(&mut self.cut_percent)
                        .range(0.0..=99.0)
                        .speed(0.1)
                        .suffix(" %"),
                );
                ui.end_row();
            });
        ui.add_space(12.0);

        let (Some(buy), Some(sell)) = (money::parse(&self.buy), money::parse(&self.sell)) else {
            ui.label("Entre un prix d'achat et un prix de vente.");
            return;
        };
        let p = money::profit(buy, sell, i64::from(self.quantity), self.cut_percent);
        let palette = theme::current(ui);
        let color = if p.total >= 0 {
            palette.gain
        } else {
            palette.loss
        };

        egui::Grid::new("result")
            .num_columns(2)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                row(
                    ui,
                    "Coût d'achat",
                    money::format(buy * i64::from(self.quantity)),
                );
                row(
                    ui,
                    "Vente brute",
                    money::format(sell * i64::from(self.quantity)),
                );
                row(
                    ui,
                    "Commission",
                    format!("-{}", money::format(p.commission)),
                );
                ui.strong("Bénéfice net");
                ui.colored_label(color, egui::RichText::new(money::format(p.total)).strong());
                ui.end_row();
                row(ui, "Par unité", money::format(p.per_unit.round() as i64));
                row(
                    ui,
                    "Rentabilité",
                    p.roi
                        .map(|r| format!("{r:+.1} %"))
                        .unwrap_or_else(|| "—".into()),
                );
                row(
                    ui,
                    "Prix de vente minimum",
                    format!("{} (pour ne rien perdre)", money::format(p.break_even)),
                );
            });
    }
}

fn money_field(ui: &mut egui::Ui, label: &str, text: &mut String) {
    ui.label(label);
    let invalid = !text.trim().is_empty() && money::parse(text).is_none();
    let edit = ui.add(egui::TextEdit::singleline(text).desired_width(140.0));
    if invalid {
        edit.on_hover_text("Format non reconnu");
        ui.colored_label(ui.visuals().error_fg_color, "?");
    } else if let Some(copper) = money::parse(text) {
        ui.weak(money::format(copper));
    } else {
        ui.label("");
    }
}

fn row(ui: &mut egui::Ui, label: &str, value: String) {
    ui.label(label);
    ui.label(value);
    ui.end_row();
}
