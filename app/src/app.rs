//! Top-level layout: header bar, side bar, content (Files-like).

use eframe::egui;

use crate::state::{Module, State, View};
#[cfg(feature = "analysis")]
use crate::ui::analysis_view;
use crate::ui::{
    calculator, chart, fonts, header, item_list, items, loot, recipes, settings, sidebar, theme,
};
use scout_core::Theme;
use scout_worker::WorkerHandle;

pub struct App {
    worker: WorkerHandle,
    state: State,
    list: item_list::ItemList,
    calculator: calculator::Calculator,
    #[cfg(feature = "analysis")]
    analysis: analysis_view::AnalysisView,
    sidebar: sidebar::Sidebar,
    recipes: recipes::Recipes,
    items: items::Items,
    loot: loot::Loot,
    settings: settings::Settings,
    /// Theme currently applied to the egui context.
    theme: Option<Theme>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        fonts::install(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        Self {
            worker: WorkerHandle::spawn(Box::new(move || ctx.request_repaint())),
            state: State::default(),
            list: item_list::ItemList::default(),
            calculator: calculator::Calculator::default(),
            #[cfg(feature = "analysis")]
            analysis: analysis_view::AnalysisView::default(),
            sidebar: sidebar::Sidebar::default(),
            recipes: recipes::Recipes::default(),
            items: items::Items::default(),
            loot: loot::Loot::default(),
            settings: settings::Settings::default(),
            theme: None,
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let events: Vec<_> = self.worker.poll().collect();
        for event in events {
            self.state.apply(event);
        }
        let wanted = self
            .state
            .config
            .as_ref()
            .map_or_else(Theme::default, |c| c.theme);
        if self.theme != Some(wanted) {
            theme::apply(ctx, wanted);
            self.theme = Some(wanted);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let Self {
            worker,
            state,
            list,
            calculator,
            #[cfg(feature = "analysis")]
            analysis,
            sidebar,
            recipes,
            items,
            loot,
            settings,
            ..
        } = self;
        let palette = theme::current(ui);
        let bar = |fill| {
            egui::Frame::new()
                .fill(fill)
                .inner_margin(egui::Margin::symmetric(12, 6))
        };

        egui::Panel::top("tabs")
            .frame(bar(palette.header))
            .show(ui, |ui| header::tabs(ui, state, settings));
        egui::Panel::bottom("status")
            .frame(bar(palette.header))
            .show(ui, |ui| header::status(ui, state));
        let content = egui::Frame::new()
            .fill(palette.view)
            .inner_margin(egui::Margin::same(14));
        match state.module {
            Module::Market => {
                egui::Panel::left("sidebar")
                    .exact_size(230.0)
                    .resizable(false)
                    .frame(
                        egui::Frame::new()
                            .fill(palette.sidebar)
                            .inner_margin(egui::Margin::symmetric(8, 10)),
                    )
                    .show(ui, |ui| sidebar.show(ui, state));
                egui::Panel::top("header")
                    .frame(bar(palette.view))
                    .show_separator_line(false)
                    .show(ui, |ui| header::show(ui, state));
                egui::CentralPanel::default()
                    .frame(content)
                    .show(ui, |ui| match state.view {
                        View::List => list.show(ui, state, worker),
                        View::Chart => chart::show(ui, state, worker),
                        #[cfg(feature = "analysis")]
                        View::Analysis => analysis.show(ui, state, worker),
                        View::Calculator => calculator.show(ui, state),
                    });
            }
            Module::Items => {
                // The module lays out its own side bar and content.
                egui::CentralPanel::default()
                    .frame(egui::Frame::new())
                    .show(ui, |ui| items.show(ui, state, worker));
            }
            Module::Recipes => {
                // The module lays out its own side bar, list and detail.
                egui::CentralPanel::default()
                    .frame(egui::Frame::new())
                    .show(ui, |ui| recipes.show(ui, state, worker));
            }
            Module::Loot => {
                // The module lays out its zones, monsters and loot.
                egui::CentralPanel::default()
                    .frame(egui::Frame::new())
                    .show(ui, |ui| loot.show(ui, state, worker));
            }
        }
        settings.window(&ui.ctx().clone(), state, worker);
    }
}
