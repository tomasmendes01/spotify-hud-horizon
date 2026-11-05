// What the game draws for the mod, as plain shapes: the helpers turn each into Horizon's
// `native_fill`, `native_quad` and `native_text` (scaled and clipped through the HUD batch).

/// One thing to draw, in GUI pixels.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    /// x, y, w, h and ARGB.
    Fill([f32; 4], u32),
    /// A line of text at its top left, opaque RGB, a shadow, a scale about its top left, and
    /// the rectangle `[x0, y0, x1, y1]` it is cut to.
    Text {
        text: String,
        at: [f32; 2],
        rgb: u32,
        shadow: bool,
        scale: f32,
        clip: Option<[f32; 4]>,
    },
}

pub type Width<'a> = &'a dyn Fn(&str) -> f32;

/// The font's line height, as vanilla's `Font.lineHeight`.
pub const LINE: f32 = 9.0;

#[derive(Default)]
pub struct Canvas {
    pub cmds: Vec<Cmd>,
}

pub fn inside(p: [f32; 2], r: [f32; 4]) -> bool {
    p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3]
}

impl Canvas {
    pub fn fill(&mut self, r: [f32; 4], argb: u32) {
        if r[2] > 0.0 && r[3] > 0.0 && argb >> 24 != 0 {
            self.cmds.push(Cmd::Fill(r, argb));
        }
    }

    /// A rectangle with rounded corners, a row of fills per GUI pixel where the corners curve.
    pub fn rounded(&mut self, [x, y, w, h]: [f32; 4], radius: f32, argb: u32) {
        let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
        if r < 0.5 {
            self.fill([x, y, w, h], argb);
            return;
        }
        let rows = r.ceil();
        for j in 0..rows as usize {
            let jf = j as f32;
            let row_h = (rows - jf).min(1.0).min(h / 2.0 - jf);
            if row_h <= 0.0 {
                break;
            }
            let d = jf + 0.5;
            let inset = if d < r { r - (r * r - (r - d) * (r - d)).max(0.0).sqrt() } else { 0.0 };
            self.fill([x + inset, y + jf, w - 2.0 * inset, row_h], argb);
            self.fill([x + inset, y + h - jf - row_h, w - 2.0 * inset, row_h], argb);
        }
        self.fill([x, y + rows, w, h - 2.0 * rows], argb);
    }

    /// `rounded` with a `border`-wide edge of `edge`.
    pub fn bordered(&mut self, [x, y, w, h]: [f32; 4], radius: f32, fill: u32, edge: u32, border: f32) {
        if edge == fill || border <= 0.0 {
            self.rounded([x, y, w, h], radius, fill);
            return;
        }
        // The edge as four bands, the fill inside them.
        self.rounded([x, y, w, border], radius.min(border), edge);
        self.rounded([x, y + h - border, w, border], radius.min(border), edge);
        self.fill([x, y + border, border, h - 2.0 * border], edge);
        self.fill([x + w - border, y + border, border, h - 2.0 * border], edge);
        self.rounded([x + border, y + border, w - 2.0 * border, h - 2.0 * border], (radius - border).max(0.0), fill);
    }

    pub fn text(&mut self, text: &str, at: [f32; 2], argb: u32, shadow: bool, scale: f32, clip: Option<[f32; 4]>) {
        if text.is_empty() {
            return;
        }
        self.cmds.push(Cmd::Text { text: text.to_owned(), at, rgb: argb & 0xFF_FFFF, shadow, scale, clip });
    }

    /// Text in a box: centred on the box's height, aligned by
    /// `align` (0 left, 0.5 centre, 1 right) and, when wider than the box with `marquee`
    /// ticks, scrolled through it with a gap of 20.
    #[allow(clippy::too_many_arguments)]
    pub fn text_in(
        &mut self,
        width: Width<'_>,
        text: &str,
        [x, y, w, h]: [f32; 4],
        align: f32,
        argb: u32,
        scale: f32,
        marquee: Option<f32>,
    ) {
        let tw = width(text);
        let (sw, sh) = (w / scale, h / scale);
        let cy = y + ((sh / 2.0).floor() - (LINE / 2.0).floor()) * scale;
        if let Some(ticks) = marquee.filter(|_| tw > sw) {
            let gap = 20f32.min(sw);
            let offset = (ticks % (tw + gap)).round();
            let clip = Some([x, y, x + w, y + h]);
            self.text(text, [x - offset * scale, cy], argb, true, scale, clip);
            self.text(text, [x + (tw + gap - offset) * scale, cy], argb, true, scale, clip);
        } else {
            let left = x + ((sw * align).round() - (tw * align).round()) * scale;
            self.text(text, [left, cy], argb, true, scale, None);
        }
    }

    /// A 10x10 icon (`ICON_*`) stretched into `[x, y, size, size]`, one fill per run.
    pub fn icon(&mut self, rows: &[&str; 10], [x, y]: [f32; 2], size: f32, argb: u32) {
        let px = size / 10.0;
        for (j, row) in rows.iter().enumerate() {
            let b = row.as_bytes();
            let mut i = 0;
            while i < b.len() {
                if b[i] == b'#' {
                    let start = i;
                    while i < b.len() && b[i] == b'#' {
                        i += 1;
                    }
                    self.fill([x + start as f32 * px, y + j as f32 * px, (i - start) as f32 * px, px], argb);
                } else {
                    i += 1;
                }
            }
        }
    }

    /// An RGB image of `side` x `side` cells drawn into `[x, y, size, size]`, runs of equal
    /// cells merged into one fill.
    pub fn image(&mut self, rgb: &[u8], side: usize, [x, y]: [f32; 2], size: f32) {
        let cell = size / side as f32;
        for row in 0..side {
            let mut i = 0;
            while i < side {
                let at = (row * side + i) * 3;
                let px = &rgb[at..at + 3];
                let start = i;
                while i < side && &rgb[(row * side + i) * 3..(row * side + i) * 3 + 3] == px {
                    i += 1;
                }
                let argb = 0xFF00_0000 | u32::from(px[0]) << 16 | u32::from(px[1]) << 8 | u32::from(px[2]);
                self.fill([x + start as f32 * cell, y + row as f32 * cell, (i - start) as f32 * cell, cell], argb);
            }
        }
    }
}

// The control icons, as 10x10 pixel art.
pub const ICON_PLAY: [&str; 10] = [
    "..........",
    "..#.......",
    "..###.....",
    "..#####...",
    "..#######.",
    "..#######.",
    "..#####...",
    "..###.....",
    "..#.......",
    "..........",
];
pub const ICON_PAUSE: [&str; 10] = [
    "..........",
    "..##..##..",
    "..##..##..",
    "..##..##..",
    "..##..##..",
    "..##..##..",
    "..##..##..",
    "..##..##..",
    "..##..##..",
    "..........",
];
pub const ICON_NEXT: [&str; 10] = [
    "..........",
    ".#.....##.",
    ".##....##.",
    ".###...##.",
    ".####..##.",
    ".####..##.",
    ".###...##.",
    ".##....##.",
    ".#.....##.",
    "..........",
];
pub const ICON_PREVIOUS: [&str; 10] = [
    "..........",
    ".##.....#.",
    ".##....##.",
    ".##...###.",
    ".##..####.",
    ".##..####.",
    ".##...###.",
    ".##....##.",
    ".##.....#.",
    "..........",
];
pub const ICON_SHUFFLE: [&str; 10] = [
    "..........",
    ".......#..",
    "##....####",
    "..#..#.#..",
    "...##.....",
    "...##.....",
    "..#..#.#..",
    "##....####",
    ".......#..",
    "..........",
];
pub const ICON_REPEAT: [&str; 10] = [
    "..........",
    "......#...",
    ".#######..",
    ".#....#...",
    ".#.......#",
    "#.......#.",
    "...#....#.",
    "..#######.",
    "...#......",
    "..........",
];
pub const ICON_EXTERNAL: [&str; 10] = [
    ".....#####",
    "........##",
    ".......#.#",
    "......#..#",
    "#####.#...",
    "#...##....",
    "#.........",
    "#........#",
    "#........#",
    "##########",
];
pub const ICON_SETTINGS: [&str; 10] = [
    "....##....",
    ".#.####.#.",
    "..######..",
    ".###..###.",
    "####..####",
    "####..####",
    ".###..###.",
    "..######..",
    ".#.####.#.",
    "....##....",
];
pub const ICON_VOLUME: [&str; 10] = [
    "..........",
    "...#......",
    "..##...#..",
    "####.#..#.",
    "####..#.#.",
    "####..#.#.",
    "####.#..#.",
    "..##...#..",
    "...#......",
    "..........",
];
pub const ICON_MUTED: [&str; 10] = [
    "..........",
    "...#......",
    "..##......",
    "####.#...#",
    "####..#.#.",
    "####...#..",
    "####..#.#.",
    "..##.#...#",
    "...#......",
    "..........",
];
pub const ICON_POSITION: [&str; 10] = [
    "....##....",
    "...####...",
    "....##....",
    ".#..##..#.",
    "##########",
    "##########",
    ".#..##..#.",
    "....##....",
    "...####...",
    "....##....",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_rows_cover_the_box() {
        let mut c = Canvas::default();
        c.rounded([0.0, 0.0, 95.0, 3.0], 3.0, 0xFFFF_FFFF);
        let area: f32 = c.cmds.iter().map(|m| if let Cmd::Fill(r, _) = m { r[2] * r[3] } else { 0.0 }).sum();
        assert!(area > 270.0 && area <= 285.0, "{area}");
        let mut c = Canvas::default();
        c.rounded([0.0, 0.0, 150.0, 50.0], 0.0, 0x7800_0000);
        assert_eq!(c.cmds.len(), 1);
    }

    #[test]
    fn icons_are_ten_by_ten() {
        for icon in [ICON_PLAY, ICON_PAUSE, ICON_NEXT, ICON_PREVIOUS, ICON_SHUFFLE, ICON_REPEAT, ICON_EXTERNAL, ICON_SETTINGS, ICON_VOLUME, ICON_MUTED, ICON_POSITION] {
            assert!(icon.iter().all(|r| r.len() == 10), "{icon:?}");
        }
    }

    #[test]
    fn marquee_scrolls_long_text_only() {
        let w = |s: &str| s.len() as f32 * 6.0;
        let mut c = Canvas::default();
        c.text_in(&w, "short", [0.0, 0.0, 95.0, 10.0], 0.0, 0xFFFFFFFF, 1.0, Some(3.0));
        assert_eq!(c.cmds.len(), 1);
        let mut c = Canvas::default();
        c.text_in(&w, "a much much longer song title", [0.0, 0.0, 95.0, 10.0], 0.0, 0xFFFFFFFF, 1.0, Some(3.0));
        assert_eq!(c.cmds.len(), 2);
        assert!(matches!(&c.cmds[0], Cmd::Text { at, clip: Some(_), .. } if at[0] == -3.0));
    }

    #[test]
    fn image_merges_runs() {
        let mut c = Canvas::default();
        c.image(&[9u8; 4 * 4 * 3], 4, [0.0, 0.0], 40.0);
        assert_eq!(c.cmds.len(), 4);
    }
}
