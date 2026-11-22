//! Clickable map of a continent: each zone in its place (client tables), with its shape (OpenMap's masks,
//! else a rectangle), colored by the usual level of its monsters.

mod paint;

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::{self, Color32, Pos2, Rect, Sense, pos2};

use crate::ui::theme::Palette;
use scout_core::drops::Zone;
use scout_core::gamedata::GameData;
use scout_core::map::{self, Frac, GameMap, Mask};

/// Continents (UiMap ids), drawn side by side: Kalimdor on the left, Eastern Kingdoms on the right.
const CONTINENTS: [i64; 2] = [1414, 1415];
/// Opacity above which a mask pixel belongs to the zone.
const INSIDE: u8 = 128;

#[derive(Default)]
pub struct WorldMap {
    /// Zone shapes as textures (white, tinted when drawn) and where their name goes.
    textures: HashMap<i64, egui::TextureHandle>,
    centers: HashMap<i64, (f64, f64)>,
    /// Address of the masks the textures were made from.
    made_from: usize,
}

/// Sort rank of a zone's continent (order of `CONTINENTS`); zones without a known continent come last.
pub fn continent_rank(game: &GameData, ui_map: Option<i64>) -> usize {
    ui_map
        .and_then(|id| game.maps.iter().find(|m| m.id == id))
        .and_then(|m| CONTINENTS.iter().position(|c| *c == m.parent))
        .unwrap_or(CONTINENTS.len())
}

/// Name of the continent of a given rank (see `continent_rank`); "Autres" for the last rank.
pub fn continent_name(game: &GameData, rank: usize) -> String {
    CONTINENTS
        .get(rank)
        .and_then(|id| game.maps.iter().find(|m| m.id == *id))
        .map_or_else(|| "Autres".to_owned(), |m| m.name.clone())
}

/// Number of continent ranks, the one for zones without a known continent included.
pub const RANKS: usize = CONTINENTS.len() + 1;

impl WorldMap {
    /// Draws the map; returns the zone clicked (its name in the zone list).
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        game: &GameData,
        zones: &[Zone],
        masks: &Arc<HashMap<i64, Mask>>,
        selected: &[String],
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
        if masks.is_empty() {
            ui.weak("Zones en rectangles : installe l'addon OpenMap pour leurs contours.");
        }
        ui.columns(CONTINENTS.len(), |columns| {
            let mut clicked = None;
            for (ui, id) in columns.iter_mut().zip(CONTINENTS) {
                let found = self.continent(ui, game, id, zones, masks, selected, palette);
                clicked = clicked.or(found);
            }
            clicked
        })
    }

    /// Draws one continent in the space left; returns the zone clicked.
    #[expect(clippy::too_many_arguments, reason = "drawing context of one map")]
    fn continent(
        &self,
        ui: &mut egui::Ui,
        game: &GameData,
        id: i64,
        zones: &[Zone],
        masks: &Arc<HashMap<i64, Mask>>,
        selected: &[String],
        palette: &Palette,
    ) -> Option<String> {
        let continent = game.maps.iter().find(|m| m.id == id)?;
        ui.vertical_centered(|ui| ui.strong(&continent.name));
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
            let strength = if zone.is_some_and(|z| selected.contains(&z.name)) {
                255
            } else if hovered.is_some_and(|h| h.id == m.id) {
                220
            } else {
                140
            };
            let color = match zone {
                Some(z) => paint::level_color(z, strength),
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
            paint::label(&painter, at, &m.name, zone, palette);
        }

        let zone = hovered.and_then(|m| by_map.get(&m.id).copied());
        if let Some(m) = hovered {
            let response = response.clone().on_hover_ui_at_pointer(|ui| {
                ui.strong(&m.name);
                match zone {
                    Some(z) => {
                        ui.label(paint::levels(z));
                        ui.weak(format!("{} monstres avec du butin", z.npcs));
                        if z.kills > 0 {
                            ui.weak(format!("{} corps fouillés (Scout)", z.kills));
                        }
                        ui.weak("Clic : ses monstres · Ctrl+clic : l'ajouter à la sélection");
                    }
                    None => {
                        ui.weak("Aucun butin connu : Forever Guide n'a pas cette zone.");
                        ui.weak("Scout l'apprendra quand tu y fouilleras des monstres (puis /reload et import).");
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
    let key = map::name_key;
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
