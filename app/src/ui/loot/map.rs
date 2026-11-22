//! Clickable map of a continent: each zone in its place (client tables), with its shape (OpenMap's masks,
//! else a rectangle), colored by the usual level of its monsters.

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, pos2};

use crate::ui::theme::Palette;
use scout_core::drops::Zone;
use scout_core::gamedata::GameData;
use scout_core::map::{self, Frac, GameMap, Mask};

/// Continents (UiMap ids), Eastern Kingdoms first.
const CONTINENTS: [i64; 2] = [1415, 1414];
/// Opacity above which a mask pixel belongs to the zone.
const INSIDE: u8 = 128;

pub struct WorldMap {
    continent: i64,
    /// Zone shapes as textures (white, tinted when drawn) and where their name goes.
    textures: HashMap<i64, egui::TextureHandle>,
    centers: HashMap<i64, (f64, f64)>,
    /// Address of the masks the textures were made from.
    made_from: usize,
}

impl Default for WorldMap {
    fn default() -> Self {
        Self {
            continent: CONTINENTS[0],
            textures: HashMap::new(),
            centers: HashMap::new(),
            made_from: 0,
        }
    }
}

impl WorldMap {
    /// Shows the continent to the zone's map (when it has one) — used when a zone is picked in the list.
    pub fn focus(&mut self, game: &GameData, ui_map: Option<i64>) {
        let parent = ui_map.and_then(|id| game.maps.iter().find(|m| m.id == id));
        if let Some(p) = parent.filter(|p| CONTINENTS.contains(&p.parent)) {
            self.continent = p.parent;
        }
    }

    /// Draws the map; returns the zone clicked (its name in the zone list).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        game: &GameData,
        zones: &[Zone],
        masks: &Arc<HashMap<i64, Mask>>,
        selected: Option<&str>,
        palette: &Palette,
    ) -> Option<String> {
        if game.maps.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label("La carte a besoin des données du jeu les plus récentes.");
                ui.weak("⚙ Réglages → mettre à jour les données du jeu (les cartes y ont été ajoutées).");
            });
            return None;
        }
        self.make_textures(ui.ctx(), masks);
        ui.horizontal(|ui| {
            for id in CONTINENTS {
                let name = game
                    .maps
                    .iter()
                    .find(|m| m.id == id)
                    .map_or("?", |m| &m.name);
                ui.selectable_value(&mut self.continent, id, name);
            }
            if masks.is_empty() {
                ui.weak("· zones en rectangles : installe l'addon OpenMap pour leurs contours");
            }
        });
        let continent = game.maps.iter().find(|m| m.id == self.continent)?;
        let shapes = map::zones_of(&game.maps, continent.id);
        let by_map = link(&shapes, zones);
        // Framed on the zones: the continent's rectangle holds a lot of sea.
        let view = Frac::around(shapes.iter().map(|m| continent.place(m)), 0.03)?;
        let rect = fit(ui.available_rect_before_wrap(), continent, &view);
        let response = ui.allocate_rect(rect, Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, palette.sidebar);

        let place = |m: &GameMap| screen(rect, continent.place(m).within(&view));
        let hovered = response.hover_pos().and_then(|pos| {
            // Smallest first: a city over its zone.
            shapes.iter().rev().copied().find(|m| {
                let r = place(m);
                let (u, v) = (
                    (pos.x - r.left()) / r.width(),
                    (pos.y - r.top()) / r.height(),
                );
                r.contains(pos)
                    && masks
                        .get(&m.id)
                        .is_none_or(|k| k.at(u.into(), v.into()) >= INSIDE)
            })
        });

        for m in &shapes {
            let zone = by_map.get(&m.id).copied();
            let strength = if selected.is_some() && zone.map(|z| z.name.as_str()) == selected {
                255
            } else if hovered.is_some_and(|h| h.id == m.id) {
                220
            } else {
                140
            };
            let color = match zone {
                Some(z) => level_color(z, strength),
                None => palette
                    .dim_text
                    .gamma_multiply(if hovered.is_some_and(|h| h.id == m.id) {
                        0.6
                    } else {
                        0.35
                    }),
            };
            let r = place(m);
            match self.textures.get(&m.id) {
                Some(t) => {
                    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                    painter.image(t.id(), r, uv, color);
                }
                None => {
                    painter.rect_filled(r.shrink(1.0), 4.0, color);
                }
            }
        }
        for m in &shapes {
            let zone = by_map.get(&m.id).copied();
            let r = place(m);
            if r.width() < 40.0 {
                continue;
            }
            let (u, v) = self.centers.get(&m.id).copied().unwrap_or((0.5, 0.5));
            let at = pos2(
                r.left() + r.width() * u as f32,
                r.top() + r.height() * v as f32,
            );
            label(&painter, at, &m.name, zone, palette);
        }

        let zone = hovered.and_then(|m| by_map.get(&m.id).copied());
        if let Some(m) = hovered {
            let response = response.clone().on_hover_ui_at_pointer(|ui| {
                ui.strong(&m.name);
                match zone {
                    Some(z) => {
                        ui.label(levels(z));
                        ui.weak(format!("{} monstres avec du butin", z.npcs));
                        if z.kills > 0 {
                            ui.weak(format!("{} corps fouillés (Scout)", z.kills));
                        }
                    }
                    None => {
                        ui.weak("Aucun butin connu");
                    }
                }
            });
            if zone.is_some() {
                response.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
        }
        if response.clicked() {
            return zone.map(|z| z.name.clone());
        }
        None
    }

    /// Turns the masks into textures once (white, the opacity being the shape).
    fn make_textures(&mut self, ctx: &egui::Context, masks: &Arc<HashMap<i64, Mask>>) {
        let address = Arc::as_ptr(masks) as usize;
        if self.made_from == address {
            return;
        }
        self.textures.clear();
        self.centers.clear();
        for (&id, mask) in masks.iter() {
            let pixels: Vec<Color32> = mask
                .alpha
                .iter()
                .map(|&a| Color32::from_white_alpha(a))
                .collect();
            let image = egui::ColorImage::new([mask.width, mask.height], pixels);
            let texture =
                ctx.load_texture(format!("zone-{id}"), image, egui::TextureOptions::LINEAR);
            self.textures.insert(id, texture);
            if let Some(center) = mask.center() {
                self.centers.insert(id, center);
            }
        }
        self.made_from = address;
    }
}

/// The zone each map shape stands for: by map id (from the drops import), else by name.
fn link<'a>(shapes: &[&GameMap], zones: &'a [Zone]) -> HashMap<i64, &'a Zone> {
    let key = |s: &str| s.to_lowercase().replace('’', "'");
    let by_name: HashMap<String, &Zone> = zones
        .iter()
        .filter(|z| !z.dungeon)
        .map(|z| (key(&z.name), z))
        .collect();
    shapes
        .iter()
        .filter_map(|m| {
            let zone = zones.iter().find(|z| z.ui_map == Some(m.id));
            Some((m.id, zone.or_else(|| by_name.get(&key(&m.name)).copied())?))
        })
        .collect()
}

/// The largest rectangle with the proportions of `view` (part of the continent), centered in `space`.
fn fit(space: Rect, continent: &GameMap, view: &Frac) -> Rect {
    let width = (continent.y_max - continent.y_min) * (view.right - view.left);
    let height = (continent.x_max - continent.x_min) * (view.bottom - view.top);
    let ratio = (width / height) as f32;
    let width = space.width().min(space.height() * ratio);
    Rect::from_center_size(space.center(), egui::vec2(width, width / ratio))
}

fn screen(rect: Rect, f: Frac) -> Rect {
    let at = |u: f64, v: f64| {
        Pos2::new(
            rect.left() + rect.width() * u as f32,
            rect.top() + rect.height() * v as f32,
        )
    };
    Rect::from_min_max(at(f.left, f.top), at(f.right, f.bottom))
}

fn levels(z: &Zone) -> String {
    match (z.min_level, z.max_level) {
        (Some(a), Some(b)) if a != b => format!("Niveaux {a}-{b}"),
        (Some(a), _) => format!("Niveau {a}"),
        _ => "Niveau inconnu".to_owned(),
    }
}

/// Green for low levels, through yellow, orange and red, to purple at 60.
fn level_color(z: &Zone, alpha: u8) -> Color32 {
    const STOPS: [(f32, [u8; 3]); 5] = [
        (1.0, [0x33, 0xd1, 0x7a]),
        (20.0, [0xf6, 0xd3, 0x2d]),
        (35.0, [0xff, 0x78, 0x00]),
        (48.0, [0xe0, 0x1b, 0x24]),
        (60.0, [0x91, 0x41, 0xac]),
    ];
    let level = match (z.min_level, z.max_level) {
        (Some(a), Some(b)) => (a + b) as f32 / 2.0,
        (Some(a), None) | (None, Some(a)) => a as f32,
        (None, None) => return Color32::from_white_alpha(alpha / 2),
    };
    let i = STOPS.iter().rposition(|(l, _)| level >= *l).unwrap_or(0);
    let (l0, c0) = STOPS[i];
    let (l1, c1) = STOPS[(i + 1).min(STOPS.len() - 1)];
    let t = if l1 > l0 {
        ((level - l0) / (l1 - l0)).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(
        mix(c0[0], c1[0]),
        mix(c0[1], c1[1]),
        mix(c0[2], c1[2]),
        alpha,
    )
}

/// Zone name and levels, with a halo so they read over any color.
fn label(painter: &egui::Painter, at: Pos2, name: &str, zone: Option<&Zone>, palette: &Palette) {
    let text = match zone.map(|z| (z.min_level, z.max_level)) {
        Some((Some(a), Some(b))) => format!("{name}\n{a}-{b}"),
        _ => name.to_owned(),
    };
    let font = egui::FontId::proportional(11.0);
    let halo = palette.view.gamma_multiply(0.8);
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
        painter.text(
            at + egui::vec2(dx, dy),
            egui::Align2::CENTER_CENTER,
            &text,
            font.clone(),
            halo,
        );
    }
    painter.text(at, egui::Align2::CENTER_CENTER, &text, font, palette.text);
}
