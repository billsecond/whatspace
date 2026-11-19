//! Drawing table rows: columns, clipped text, share bars and the small folder and file icons.
//! Every table in the window is painted row by row like this, so only the rows on screen cost
//! anything however many there are.

use egui::{Align2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};

/// Height of one row.
pub const ROW_H: f32 = 22.0;

pub fn font() -> FontId {
    FontId::proportional(13.0)
}

pub fn small_font() -> FontId {
    FontId::proportional(11.5)
}

/// A column: its heading, width and which side its text keeps to.
#[derive(Clone, Copy, Debug)]
pub struct Column {
    pub title: &'static str,
    pub width: f32,
    pub right: bool,
}

impl Column {
    pub const fn left(title: &'static str, width: f32) -> Column {
        Column { title, width, right: false }
    }

    pub const fn right(title: &'static str, width: f32) -> Column {
        Column { title, width, right: true }
    }
}

/// Split a row into a stretching first column and the fixed `columns` after it. Columns that
/// don't fit, starting with the last, get no space.
pub fn split(row: Rect, columns: &[Column], min_first: f32) -> (Rect, Vec<Rect>) {
    let mut avail = (row.width() - min_first).max(0.0);
    let shown: Vec<bool> = columns
        .iter()
        .map(|c| {
            let fits = c.width <= avail;
            if fits {
                avail -= c.width;
            }
            fits
        })
        .collect();
    let fixed: f32 = columns.iter().zip(&shown).filter(|(_, s)| **s).map(|(c, _)| c.width).sum();
    let first = Rect::from_min_size(row.min, vec2((row.width() - fixed).max(0.0), row.height()));
    let mut x = first.max.x;
    let rects = columns
        .iter()
        .zip(&shown)
        .map(|(c, s)| {
            let w = if *s { c.width } else { 0.0 };
            let r = Rect::from_min_size(pos2(x, row.min.y), vec2(w, row.height()));
            x += w;
            r
        })
        .collect();
    (first, rects)
}

/// Text at the left of `rect`, cut off at its right edge. Returns where it was drawn.
pub fn text_left(p: &Painter, rect: Rect, text: &str, color: Color32, font: FontId) -> Option<Rect> {
    if rect.width() <= 2.0 {
        return None;
    }
    Some(p.with_clip_rect(rect.intersect(p.clip_rect())).text(pos2(rect.min.x + 4.0, rect.center().y), Align2::LEFT_CENTER, text, font, color))
}

/// Text at the right of `rect`, cut off at its left edge. Returns where it was drawn.
pub fn text_right(p: &Painter, rect: Rect, text: &str, color: Color32, font: FontId) -> Option<Rect> {
    if rect.width() <= 2.0 {
        return None;
    }
    Some(p.with_clip_rect(rect.intersect(p.clip_rect())).text(pos2(rect.max.x - 6.0, rect.center().y), Align2::RIGHT_CENTER, text, font, color))
}

/// A share bar filling `fraction` of `rect` (inset a little), with the percentage written on it.
pub fn share_bar(p: &Painter, rect: Rect, fraction: f32, fill: Color32, track: Color32, label: &str, text: Color32) {
    if rect.width() < 12.0 {
        return;
    }
    let bar = rect.shrink2(vec2(4.0, 5.0));
    p.rect_filled(bar, 2.0, track);
    let f = if fraction.is_finite() { fraction.clamp(0.0, 1.0) } else { 0.0 };
    if f > 0.0 {
        let mut filled = bar;
        filled.set_width((bar.width() * f).max(1.5));
        p.rect_filled(filled, 2.0, fill);
    }
    p.with_clip_rect(rect.intersect(p.clip_rect())).text(pos2(bar.max.x - 3.0, bar.center().y), Align2::RIGHT_CENTER, label, small_font(), text);
}

/// The open/closed triangle in front of a folder that has something in it.
pub fn expander(p: &Painter, center: Pos2, open: bool, color: Color32) {
    let s = 3.6;
    let points = if open {
        vec![pos2(center.x - s, center.y - s * 0.5), pos2(center.x + s, center.y - s * 0.5), pos2(center.x, center.y + s * 0.7)]
    } else {
        vec![pos2(center.x - s * 0.5, center.y - s), pos2(center.x + s * 0.7, center.y), pos2(center.x - s * 0.5, center.y + s)]
    };
    p.add(Shape::convex_polygon(points, color, Stroke::NONE));
}

/// A folder icon centred on `center`.
pub fn folder_icon(p: &Painter, center: Pos2, color: Color32) {
    let body = Rect::from_center_size(center + vec2(0.0, 1.0), vec2(14.0, 10.0));
    let tab = Rect::from_min_size(body.min - vec2(0.0, 2.5), vec2(6.0, 3.5));
    p.rect_filled(tab, 1.0, color);
    p.rect_filled(body, 1.5, color);
    p.line_segment(
        [pos2(body.min.x + 1.0, body.min.y + 2.5), pos2(body.max.x - 1.0, body.min.y + 2.5)],
        Stroke::new(1.0, Color32::from_black_alpha(40)),
    );
}

/// A file icon (a page with its corner turned down) centred on `center`.
pub fn file_icon(p: &Painter, center: Pos2, color: Color32, outline: Color32) {
    let r = Rect::from_center_size(center, vec2(10.0, 13.0));
    let fold = 3.5;
    let points = vec![r.left_top(), pos2(r.max.x - fold, r.min.y), pos2(r.max.x, r.min.y + fold), r.right_bottom(), r.left_bottom()];
    p.add(Shape::convex_polygon(points, color, Stroke::new(1.0, outline)));
}

/// A link icon: an arrow in a box.
pub fn link_icon(p: &Painter, center: Pos2, color: Color32) {
    let r = Rect::from_center_size(center, vec2(11.0, 11.0));
    p.rect_stroke(r, 2.0, Stroke::new(1.2, color), StrokeKind::Inside);
    p.line_segment([pos2(r.min.x + 3.0, r.max.y - 3.0), pos2(r.max.x - 3.0, r.min.y + 3.0)], Stroke::new(1.2, color));
    p.line_segment([pos2(r.max.x - 6.0, r.min.y + 3.0), pos2(r.max.x - 3.0, r.min.y + 3.0)], Stroke::new(1.2, color));
    p.line_segment([pos2(r.max.x - 3.0, r.min.y + 3.0), pos2(r.max.x - 3.0, r.min.y + 6.0)], Stroke::new(1.2, color));
}

/// A heading row for `columns` after a first column titled `first`. Returns which heading was
/// clicked: 0 for the first column, 1… for the others.
pub fn header(ui: &mut egui::Ui, first: &str, columns: &[Column], sorted: Option<(usize, bool)>, text: Color32, line: Color32) -> Option<usize> {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), egui::Sense::hover());
    let (first_rect, rects) = split(rect, columns, 140.0);
    let p = ui.painter();
    p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, line));
    let mut clicked = None;
    let all = std::iter::once((first_rect, first, false)).chain(rects.iter().zip(columns).map(|(r, c)| (*r, c.title, c.right)));
    for (i, (r, title, right)) in all.enumerate() {
        if r.width() <= 2.0 {
            continue;
        }
        // The sorted column shows which way with a small triangle beside its heading.
        let arrow = match sorted {
            Some((col, desc)) if col == i => Some(desc),
            _ => None,
        };
        let shown = if right && arrow.is_some() { Rect::from_min_max(r.min, pos2(r.max.x - 10.0, r.max.y)) } else { r };
        let drawn = if right { text_right(p, shown, title, text, small_font()) } else { text_left(p, shown, title, text, small_font()) };
        if let (Some(desc), Some(drawn)) = (arrow, drawn) {
            let c = pos2(drawn.max.x + 7.0, drawn.center().y);
            let s = 3.0;
            let points = if desc {
                vec![pos2(c.x - s, c.y - s * 0.6), pos2(c.x + s, c.y - s * 0.6), pos2(c.x, c.y + s * 0.8)]
            } else {
                vec![pos2(c.x - s, c.y + s * 0.6), pos2(c.x + s, c.y + s * 0.6), pos2(c.x, c.y - s * 0.8)]
            };
            p.add(Shape::convex_polygon(points, text, Stroke::NONE));
        }
        if sorted.is_some() && ui.interact(r, ui.id().with(("header", i)), egui::Sense::click()).clicked() {
            clicked = Some(i);
        }
    }
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_drop_from_the_end_when_narrow() {
        let cols = [Column::right("a", 100.0), Column::right("b", 100.0), Column::right("c", 100.0)];
        let (first, rects) = split(Rect::from_min_size(Pos2::ZERO, vec2(500.0, 20.0)), &cols, 140.0);
        assert_eq!(first.width(), 300.0 - 100.0);
        assert_eq!(rects.iter().filter(|r| r.width() > 0.0).count(), 3);
        let (first, rects) = split(Rect::from_min_size(Pos2::ZERO, vec2(300.0, 20.0)), &cols, 140.0);
        assert_eq!(rects.iter().filter(|r| r.width() > 0.0).count(), 1);
        assert_eq!(first.width(), 200.0);
    }
}
