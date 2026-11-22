//! The game's maps (client tables UiMap and UiMapAssignment): continents and zones with the rectangle they
//! cover in world coordinates, to draw each zone in its place on its continent.
//!
//! World coordinates: `x` grows to the north, `y` to the west. On a map, left to right is decreasing `y`
//! and top to bottom decreasing `x`.

/// UiMap type of a continent.
pub const CONTINENT: i64 = 2;
/// UiMap type of a zone (and of a city).
pub const ZONE: i64 = 3;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "sqlx", derive(sqlx::FromRow))]
pub struct GameMap {
    /// UiMap id (e.g. 1431 Bois de la Pénombre, 1415 Royaumes de l'est).
    pub id: i64,
    pub name: String,
    pub parent: i64,
    /// UiMap type: 2 continent, 3 zone, …
    pub kind: i64,
    /// World area it covers.
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
}

/// A rectangle as fractions of a map (0..1, origin at the top left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frac {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Frac {
    /// Smallest rectangle holding all of them, widened by `margin` (fraction of its size) on each side.
    pub fn around(fracs: impl IntoIterator<Item = Frac>, margin: f64) -> Option<Frac> {
        let b = fracs.into_iter().reduce(|a, b| Frac {
            left: a.left.min(b.left),
            top: a.top.min(b.top),
            right: a.right.max(b.right),
            bottom: a.bottom.max(b.bottom),
        })?;
        let (w, h) = (b.right - b.left, b.bottom - b.top);
        Some(Frac {
            left: b.left - w * margin,
            top: b.top - h * margin,
            right: b.right + w * margin,
            bottom: b.bottom + h * margin,
        })
    }

    /// This rectangle as fractions of `view` (a part of the same map).
    pub fn within(&self, view: &Frac) -> Frac {
        let h = |u: f64| (u - view.left) / (view.right - view.left);
        let v = |y: f64| (y - view.top) / (view.bottom - view.top);
        Frac {
            left: h(self.left),
            top: v(self.top),
            right: h(self.right),
            bottom: v(self.bottom),
        }
    }
}

impl GameMap {
    /// Where `other` lies on this map.
    pub fn place(&self, other: &GameMap) -> Frac {
        let h = |y: f64| (self.y_max - y) / (self.y_max - self.y_min);
        let v = |x: f64| (self.x_max - x) / (self.x_max - self.x_min);
        Frac {
            left: h(other.y_max),
            right: h(other.y_min),
            top: v(other.x_max),
            bottom: v(other.x_min),
        }
    }
}

/// The zones drawn on a continent: its direct children of type zone, largest first (so smaller ones, like
/// cities, are drawn over them).
pub fn zones_of(maps: &[GameMap], continent: i64) -> Vec<&GameMap> {
    let mut zones: Vec<&GameMap> = maps
        .iter()
        .filter(|m| m.parent == continent && m.kind == ZONE)
        .collect();
    zones.sort_by(|a, b| area(b).total_cmp(&area(a)).then(a.id.cmp(&b.id)));
    zones
}

fn area(m: &GameMap) -> f64 {
    (m.x_max - m.x_min) * (m.y_max - m.y_min)
}

/// A zone's shape on its own map: opacity per pixel, rows from the top (OpenMap's `MapTerritory` masks).
#[derive(Debug, Clone, PartialEq)]
pub struct Mask {
    pub width: usize,
    pub height: usize,
    pub alpha: Vec<u8>,
}

impl Mask {
    /// Opacity at a point of the zone's map (fractions, origin at the top left); 0 outside.
    pub fn at(&self, u: f64, v: f64) -> u8 {
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return 0;
        }
        let x = (u * self.width as f64) as usize;
        let y = (v * self.height as f64) as usize;
        self.alpha[y * self.width + x]
    }

    /// Middle of the opaque pixels (fractions), where the zone's name goes; None for an empty mask.
    pub fn center(&self) -> Option<(f64, f64)> {
        let (mut sx, mut sy, mut n) = (0.0, 0.0, 0.0);
        for (i, &a) in self.alpha.iter().enumerate() {
            if a > 127 {
                sx += (i % self.width) as f64 + 0.5;
                sy += (i / self.width) as f64 + 0.5;
                n += 1.0;
            }
        }
        (n > 0.0).then(|| (sx / n / self.width as f64, sy / n / self.height as f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(id: i64, parent: i64, kind: i64, x: (f64, f64), y: (f64, f64)) -> GameMap {
        GameMap {
            id,
            name: format!("carte {id}"),
            parent,
            kind,
            x_min: x.0,
            x_max: x.1,
            y_min: y.0,
            y_max: y.1,
        }
    }

    #[test]
    fn places_a_zone_on_its_continent() {
        // Client values: Eastern Kingdoms and Duskwood.
        let kingdoms = map(
            1415,
            947,
            CONTINENT,
            (-16000.0, 7466.6),
            (-19199.9, 16000.0),
        );
        let duskwood = map(1431, 1415, ZONE, (-11516.7, -9716.7), (-1866.7, 833.3));
        let p = kingdoms.place(&duskwood);
        assert!((p.left - 0.431).abs() < 0.001, "{p:?}");
        assert!((p.right - 0.508).abs() < 0.001, "{p:?}");
        assert!((p.top - 0.732).abs() < 0.001, "{p:?}");
        assert!((p.bottom - 0.809).abs() < 0.001, "{p:?}");
    }

    #[test]
    fn lists_zones_largest_first() {
        let maps = vec![
            map(1453, 1415, ZONE, (0.0, 10.0), (0.0, 10.0)),
            map(1429, 1415, ZONE, (0.0, 100.0), (0.0, 100.0)),
            map(1459, 1424, 6, (0.0, 50.0), (0.0, 50.0)),
            map(1411, 1414, ZONE, (0.0, 100.0), (0.0, 100.0)),
        ];
        let ids: Vec<i64> = zones_of(&maps, 1415).iter().map(|m| m.id).collect();
        assert_eq!(ids, [1429, 1453]);
    }

    #[test]
    fn frames_the_zones() {
        let a = Frac {
            left: 0.4,
            top: 0.2,
            right: 0.5,
            bottom: 0.4,
        };
        let b = Frac {
            left: 0.45,
            top: 0.3,
            right: 0.6,
            bottom: 0.6,
        };
        let view = Frac::around([a, b], 0.0).unwrap();
        assert_eq!(
            view,
            Frac {
                left: 0.4,
                top: 0.2,
                right: 0.6,
                bottom: 0.6
            }
        );
        let inner = b.within(&view);
        assert!((inner.left - 0.25).abs() < 1e-9 && (inner.bottom - 1.0).abs() < 1e-9);
        assert_eq!(Frac::around([], 0.1), None);
    }

    #[test]
    fn reads_a_mask() {
        let mask = Mask {
            width: 2,
            height: 2,
            alpha: vec![0, 255, 10, 20],
        };
        assert_eq!(mask.at(0.75, 0.25), 255);
        assert_eq!(mask.at(0.25, 0.75), 10);
        assert_eq!(mask.at(1.0, 0.5), 0);
        assert_eq!(mask.center(), Some((0.75, 0.25)));
    }
}
