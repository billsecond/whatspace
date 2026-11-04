//! The treemap: every folder a rectangle, divided among what is in it in proportion to size.
//!
//! The rectangles are laid out with the squarified algorithm (Bruls, Huizing and van Wijk,
//! "Squarified Treemaps", 2000), which keeps them close to square so they are easy to compare
//! and to point at.

use crate::tree::{Kind, Metric, NodeId, Tree};

/// A rectangle in screen units.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w: w.max(0.0), h: h.max(0.0) }
    }

    pub fn area(&self) -> f32 {
        self.w * self.h
    }

    /// Smaller by `left`, `top`, `right` and `bottom`, never below nothing.
    pub fn inset(&self, left: f32, top: f32, right: f32, bottom: f32) -> Rect {
        Rect::new(self.x + left, self.y + top, self.w - left - right, self.h - top - bottom)
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }
}

/// Divide `area` among `values` (largest first works best) in proportion, as near to squares as
/// can be. Returns one rectangle per value, in the same order; a value of nothing (or less) gets
/// an empty rectangle.
pub fn squarify(values: &[f64], area: Rect) -> Vec<Rect> {
    let mut out = vec![Rect::new(area.x, area.y, 0.0, 0.0); values.len()];
    let total: f64 = values.iter().filter(|v| v.is_finite() && **v > 0.0).sum();
    if total <= 0.0 || area.w <= 0.0 || area.h <= 0.0 {
        return out;
    }
    let scale = f64::from(area.w) * f64::from(area.h) / total;
    // The values with something to show, as areas, with where they go.
    let items: Vec<(usize, f64)> = values.iter().enumerate().filter(|(_, v)| v.is_finite() && **v > 0.0).map(|(i, v)| (i, v * scale)).collect();
    let (mut x, mut y, mut w, mut h) = (f64::from(area.x), f64::from(area.y), f64::from(area.w), f64::from(area.h));
    let mut start = 0;
    while start < items.len() {
        let side = w.min(h);
        if side <= 0.0 {
            break;
        }
        // Grow the row while that makes its worst aspect ratio better.
        let mut end = start + 1;
        let mut sum = items.get(start).map_or(0.0, |it| it.1);
        let mut worst = worst_ratio(items.get(start..end).unwrap_or(&[]), sum, side);
        while let Some(next) = items.get(end) {
            let grown = sum + next.1;
            let ratio = worst_ratio(items.get(start..=end).unwrap_or(&[]), grown, side);
            if ratio > worst {
                break;
            }
            worst = ratio;
            sum = grown;
            end += 1;
        }
        let thickness = if side > 0.0 { sum / side } else { 0.0 };
        let row = items.get(start..end).unwrap_or(&[]);
        if w >= h {
            // A column at the left, its items stacked from the top.
            let mut cy = y;
            for (i, a) in row {
                let ih = if thickness > 0.0 { a / thickness } else { 0.0 };
                if let Some(slot) = out.get_mut(*i) {
                    *slot = Rect::new(x as f32, cy as f32, thickness as f32, ih as f32);
                }
                cy += ih;
            }
            x += thickness;
            w = (w - thickness).max(0.0);
        } else {
            // A row along the top, its items side by side.
            let mut cx = x;
            for (i, a) in row {
                let iw = if thickness > 0.0 { a / thickness } else { 0.0 };
                if let Some(slot) = out.get_mut(*i) {
                    *slot = Rect::new(cx as f32, y as f32, iw as f32, thickness as f32);
                }
                cx += iw;
            }
            y += thickness;
            h = (h - thickness).max(0.0);
        }
        start = end;
    }
    out
}

/// The worst (largest) aspect ratio in a row of areas `row` adding up to `sum`, laid along `side`.
fn worst_ratio(row: &[(usize, f64)], sum: f64, side: f64) -> f64 {
    if row.is_empty() || sum <= 0.0 || side <= 0.0 {
        return f64::INFINITY;
    }
    let (min, max) = row.iter().fold((f64::INFINITY, 0.0f64), |(lo, hi), (_, a)| (lo.min(*a), hi.max(*a)));
    let (s2, w2) = (sum * sum, side * side);
    (w2 * max / s2).max(s2 / (w2 * min))
}

/// One rectangle of a treemap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    pub rect: Rect,
    pub node: NodeId,
    /// Levels below the treemap's root (0 for the root).
    pub depth: u16,
    pub kind: Kind,
}

/// How a treemap is laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub metric: Metric,
    /// Space between a folder's edge and what is in it.
    pub padding: f32,
    /// Space at the top of a folder for its name, when it is tall enough to have one.
    pub header: f32,
    /// A folder smaller than this either way is not divided further.
    pub min_side: f32,
    /// At most this many tiles (the rest is too small to see anyway).
    pub max_tiles: usize,
    /// At most this many levels deep.
    pub max_depth: u16,
}

impl Default for Options {
    fn default() -> Self {
        Options { metric: Metric::Size, padding: 2.0, header: 16.0, min_side: 6.0, max_tiles: 60_000, max_depth: 64 }
    }
}

/// The tiles of the treemap of `root` in `area`: parents before their children, so drawing them
/// in order puts every tile over its folder.
pub fn layout(tree: &Tree, root: NodeId, area: Rect, opts: &Options) -> Vec<Tile> {
    let mut tiles = Vec::new();
    let Some(node) = tree.live(root) else { return tiles };
    let mut stack = vec![(root, area, 0u16, node.kind)];
    while let Some((id, rect, depth, kind)) = stack.pop() {
        tiles.push(Tile { rect, node: id, depth, kind });
        if kind != Kind::Dir || depth >= opts.max_depth || tiles.len() >= opts.max_tiles {
            continue;
        }
        let header = if rect.h >= opts.header * 3.0 && rect.w >= opts.header * 3.0 { opts.header } else { 0.0 };
        let inner = rect.inset(opts.padding, opts.padding + header, opts.padding, opts.padding);
        if inner.w < opts.min_side || inner.h < opts.min_side {
            continue;
        }
        let kids: Vec<NodeId> =
            tree.sorted_children(id, opts.metric).into_iter().filter(|k| tree.node(*k).is_some_and(|n| n.metric(opts.metric) > 0)).collect();
        let values: Vec<f64> = kids.iter().map(|k| tree.node(*k).map_or(0.0, |n| n.metric(opts.metric) as f64)).collect();
        let rects = squarify(&values, inner);
        // Pushed smallest first so the largest come off the stack first.
        for (kid, r) in kids.iter().zip(rects).rev() {
            if r.w >= 1.0
                && r.h >= 1.0
                && let Some(n) = tree.node(*kid)
            {
                stack.push((*kid, r, depth.saturating_add(1), n.kind));
            }
        }
    }
    tiles
}

/// The deepest tile under a point.
pub fn hit(tiles: &[Tile], x: f32, y: f32) -> Option<&Tile> {
    tiles.iter().rev().find(|t| t.rect.contains(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Entry;
    use proptest::prelude::*;

    fn overlap(a: &Rect, b: &Rect) -> f32 {
        let w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
        let h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
        if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
    }

    #[test]
    fn areas_are_in_proportion() {
        let area = Rect::new(0.0, 0.0, 600.0, 400.0);
        let rects = squarify(&[6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0], area);
        let total = 24.0;
        for (r, v) in rects.iter().zip([6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0]) {
            let expected = area.area() * v / total;
            assert!((r.area() - expected).abs() < 1.0, "{r:?} should be about {expected}");
        }
        // The paper's example: the first two make a column of near squares.
        assert!((rects[0].w / rects[0].h - 1.0).abs() < 0.6, "{:?}", rects[0]);
    }

    #[test]
    fn nothing_and_odd_values() {
        let area = Rect::new(10.0, 10.0, 100.0, 50.0);
        assert!(squarify(&[], area).is_empty());
        let r = squarify(&[0.0, 5.0, f64::NAN, -3.0, 5.0], area);
        assert_eq!(r.len(), 5);
        assert_eq!(r[0].area(), 0.0);
        assert_eq!(r[2].area(), 0.0);
        assert_eq!(r[3].area(), 0.0);
        assert!((r[1].area() - 2500.0).abs() < 1.0 && (r[4].area() - 2500.0).abs() < 1.0);
        assert!(squarify(&[1.0], Rect::new(0.0, 0.0, 0.0, 10.0)).iter().all(|r| r.area() == 0.0));
    }

    #[test]
    fn lays_out_a_tree_parents_first() {
        let mut t = Tree::new("/d", 0);
        let ids = t.add_children(NodeId::ROOT, vec![Entry::dir("big", 0), Entry::file("small.bin", 1000, 1000, 0), Entry::file("empty", 0, 0, 0)]);
        t.add_children(ids[0], vec![Entry::file("x", 6000, 6000, 0), Entry::file("y", 3000, 3000, 0)]);
        let tiles = layout(&t, NodeId::ROOT, Rect::new(0.0, 0.0, 800.0, 600.0), &Options::default());
        assert_eq!(tiles[0].node, NodeId::ROOT);
        assert_eq!(tiles.len(), 5, "root, big, x, y, small (not the empty file)");
        let pos = |id: NodeId| tiles.iter().position(|t| t.node == id).unwrap();
        assert!(pos(ids[0]) < tiles.iter().position(|t| t.depth == 2).unwrap());
        let hit_tile = hit(&tiles, tiles[pos(ids[1])].rect.x + 1.0, tiles[pos(ids[1])].rect.y + 1.0).unwrap();
        assert_eq!(hit_tile.node, ids[1]);
        assert!(layout(&t, NodeId::from_index(77).unwrap(), Rect::new(0.0, 0.0, 10.0, 10.0), &Options::default()).is_empty());
    }

    proptest! {
        #[test]
        fn rectangles_stay_inside_and_never_overlap(values in proptest::collection::vec(0.0f64..1e9, 1..40), w in 1.0f32..2000.0, h in 1.0f32..2000.0) {
            let mut values = values;
            values.sort_by(|a, b| b.total_cmp(a));
            let area = Rect::new(5.0, 7.0, w, h);
            let rects = squarify(&values, area);
            prop_assert_eq!(rects.len(), values.len());
            let tolerance = 0.01 * (w + h);
            for r in &rects {
                prop_assert!(r.x >= area.x - tolerance && r.y >= area.y - tolerance);
                prop_assert!(r.x + r.w <= area.x + area.w + tolerance && r.y + r.h <= area.y + area.h + tolerance, "{:?} in {:?}", r, area);
            }
            let total_overlap: f32 = rects.iter().enumerate().flat_map(|(i, a)| rects.iter().skip(i + 1).map(move |b| overlap(a, b))).sum();
            prop_assert!(total_overlap <= 0.001 * area.area() + 1.0, "overlap {}", total_overlap);
        }
    }
}
