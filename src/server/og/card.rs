//! The preview image: a card of washi paper, the name or today's daily on one side, a tilted
//! board on the other, drawn as SVG and rasterized with resvg.

use std::fmt::Write as _;

use resvg::tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};
use resvg::usvg::{self, Node, Options};

use super::fonts::{Family, Fonts};

pub(super) const WIDTH: u32 = 1200;
pub(super) const HEIGHT: u32 = 630;
/// [`WIDTH`] for coordinates.
const RIGHT: f32 = 1200.0;

// The site's colours (style/tailwind.css, day theme)
const INK: &str = "#2b2420";
const MUTED: &str = "#776a5c";
const SHU: &str = "#cf3a24";
const PAPER: &str = "#f4ecdd";
const CARD: &str = "#fbf6ec";
const BORDER: &str = "#dccdb1";
const ON_SHU: &str = "#fff7ee";
const WOOD: &str = "#c88a4c";
const WOOD_DARK: &str = "#74471f";

/// Where the text starts, from the left (right in right-to-left languages).
const MARGIN: f32 = 72.0;
/// The width of the text column.
const COLUMN: f32 = 580.0;

/// A board as drawn on the site, in art pixels (16 per cell).
#[derive(Clone, Debug)]
pub(super) struct BoardArt {
    /// The inside of its `<svg>`, styled
    pub(super) content: String,
    /// Its `viewBox`, `x y width height`
    pub(super) view_box: [i32; 4],
}

#[derive(Clone, Debug)]
pub(super) enum Content {
    /// The site: its name, what it is, three chips.
    Site { tagline: String, chips: [String; 3] },
    /// Today's daily: number, date, difficulty and size, never its puzzle.
    Daily {
        weekday_kanji: &'static str,
        title: String,
        date: String,
        difficulty_kanji: &'static str,
        difficulty: String,
        size: String,
        tagline: String,
    },
}

#[derive(Clone, Debug)]
pub(super) struct Card {
    pub(super) content: Content,
    /// Right-to-left: the text on the right, the board on the left
    pub(super) rtl: bool,
    /// The font of the language's text
    pub(super) text_font: Family,
    /// The font of its headings
    pub(super) title_font: Family,
    pub(super) board: BoardArt,
}

/// A number of things, for coordinates.
fn count(n: usize) -> f32 {
    u16::try_from(n).map_or(f32::MAX, f32::from)
}

/// Pixels, for coordinates.
fn px(n: u32) -> f32 {
    u16::try_from(n).map_or(f32::MAX, f32::from)
}

/// One line of text, laid out.
struct Line {
    markup: String,
    width: f32,
}

/// Builds the card's SVG.
struct Layout<'a> {
    fonts: &'a Fonts,
    options: &'a Options<'static>,
    card: &'a Card,
    svg: String,
}

impl Layout<'_> {
    fn text_chain(&self) -> [Family; 2] {
        [self.card.text_font, Family::Murecho]
    }

    fn title_chain(&self) -> [Family; 2] {
        [self.card.title_font, Family::Murecho]
    }

    /// `x` from the start of the text: from the left, or from the right in right-to-left text.
    fn x(&self, x: f32) -> f32 {
        if self.card.rtl { RIGHT - x } else { x }
    }

    fn anchor(&self) -> &'static str {
        if self.card.rtl { "end" } else { "start" }
    }

    /// Text in spans of faces that have its characters. Right-to-left text in an embedding, so
    /// numbers and Latin words in it stand where a reader of the language expects them.
    fn markup(&self, text: &str, families: &[Family], weight: u16) -> String {
        if self.card.rtl {
            self.fonts
                .spans(&format!("\u{202b}{text}\u{202c}"), families, weight)
        } else {
            self.fonts.spans(text, families, weight)
        }
    }

    /// Width of a text, as resvg lays it out.
    fn measure(&self, markup: &str, size: f32, weight: u16) -> f32 {
        let svg = format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{HEIGHT}\">\
             <text x=\"0\" y=\"100\" font-size=\"{size}\" font-weight=\"{weight}\">{markup}</text></svg>"
        );
        usvg::Tree::from_str(&svg, self.options)
            .ok()
            .and_then(|tree| match tree.root().children().first() {
                Some(Node::Text(text)) => Some(text.bounding_box().width()),
                _ => None,
            })
            .unwrap_or(0.0)
    }

    fn line(&self, text: &str, families: &[Family], size: f32, weight: u16) -> Line {
        let markup = self.markup(text, families, weight);
        let width = self.measure(&markup, size, weight);
        Line { markup, width }
    }

    /// `text` in at most `max_lines` lines of at most `width`, at `size` or as much smaller as it
    /// takes. Breaks at spaces, and between CJK characters.
    fn wrap(
        &self,
        text: &str,
        families: &[Family],
        mut size: f32,
        weight: u16,
        width: f32,
        max_lines: usize,
    ) -> (Vec<Line>, f32) {
        let words = break_points(text);
        loop {
            let whole = self.line(text, families, size, weight);
            if whole.width <= width {
                return (vec![whole], size);
            }
            // Two phrases where a comma divides them, when both fit
            if max_lines >= 2
                && let Some(phrases) = phrases(text)
            {
                let lines = phrases.map(|t| self.line(t, families, size, weight));
                if lines.iter().all(|l| l.width <= width) {
                    return (lines.into(), size);
                }
            }
            let mut lines: Vec<Line> = Vec::new();
            let mut current = String::new();
            for word in &words {
                let candidate = format!("{current}{word}");
                if current.is_empty()
                    || self.line(candidate.trim(), families, size, weight).width <= width
                {
                    current = candidate;
                } else {
                    lines.push(self.line(current.trim(), families, size, weight));
                    current = word.trim_start().to_string();
                }
            }
            lines.push(self.line(current.trim(), families, size, weight));
            let fits = lines.len() <= max_lines && lines.iter().all(|l| l.width <= width);
            if fits || size < 12.0 {
                if lines.len() > max_lines {
                    // Too long even small: in one line, squeezed by the caller's width
                    lines = vec![self.line(text, families, size, weight)];
                }
                return (lines, size);
            }
            size *= 0.94;
        }
    }

    fn text(&mut self, x: f32, y: f32, size: f32, weight: u16, fill: &str, markup: &str) {
        let _ = write!(
            self.svg,
            "<text x=\"{x}\" y=\"{y}\" font-size=\"{size}\" font-weight=\"{weight}\" fill=\"{fill}\" text-anchor=\"{}\">{markup}</text>",
            self.anchor()
        );
    }

    /// Text that starts at `x` from the start of the line, squeezed when it is wider than `max`.
    #[expect(clippy::too_many_arguments, reason = "where, how and what")]
    fn line_at(
        &mut self,
        x: f32,
        y: f32,
        size: f32,
        weight: u16,
        fill: &str,
        line: &Line,
        max: f32,
    ) {
        let x = self.x(x);
        if line.width > max {
            let anchor = self.anchor();
            let _ = write!(
                self.svg,
                "<text x=\"{x}\" y=\"{y}\" font-size=\"{size}\" font-weight=\"{weight}\" fill=\"{fill}\" text-anchor=\"{anchor}\" textLength=\"{max}\" lengthAdjust=\"spacingAndGlyphs\">{}</text>",
                line.markup
            );
        } else {
            self.text(x, y, size, weight, fill, &line.markup);
        }
    }

    /// The hanko 判子 seal of the logo, a kanji on vermilion, tilted.
    fn hanko(&mut self, x: f32, y: f32, size: f32, kanji: &str) {
        let x = if self.card.rtl { RIGHT - x - size } else { x };
        let (cx, cy) = (x + size / 2.0, y + size / 2.0);
        let angle = if self.card.rtl { 5.0 } else { -5.0 };
        let r = size * 0.14;
        let inset = size * 0.06;
        let markup = self.fonts.spans(kanji, &[Family::Pixel], 400);
        let font = size * 0.62;
        let _ = write!(
            self.svg,
            "<g transform=\"rotate({angle} {cx} {cy})\">\
             <rect x=\"{}\" y=\"{}\" width=\"{size}\" height=\"{size}\" rx=\"{r}\" fill=\"#8e2415\" opacity=\"0.25\"/>\
             <rect x=\"{x}\" y=\"{y}\" width=\"{size}\" height=\"{size}\" rx=\"{r}\" fill=\"{SHU}\"/>\
             <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\" fill=\"none\" stroke=\"{ON_SHU}\" stroke-width=\"{}\"/>\
             <text x=\"{cx}\" y=\"{}\" font-size=\"{font}\" fill=\"{ON_SHU}\" text-anchor=\"middle\">{markup}</text></g>",
            x + 3.0,
            y + 5.0,
            x + inset,
            y + inset,
            size - 2.0 * inset,
            size - 2.0 * inset,
            r * 0.7,
            size * 0.035,
            cy + font * 0.36,
        );
    }

    /// Chips in a row from the start of the line, the first one vermilion if `first_shu`. A chip
    /// is texts side
    /// by side, each in its own `<text>`: resvg loses letters of joined scripts in a text of
    /// several fonts.
    fn chips(&mut self, y: f32, chips: &[Vec<Line>], size: f32, first_shu: bool) {
        let height = size * 2.3;
        let pad = size * 0.9;
        let gap = size * 0.6;
        let space = size * 0.35;
        let mut x = MARGIN;
        for (i, parts) in chips.iter().enumerate() {
            let inner = parts.iter().map(|p| p.width).sum::<f32>()
                + space * count(parts.len().saturating_sub(1));
            let w = inner + 2.0 * pad;
            let left = if self.card.rtl { RIGHT - x - w } else { x };
            let (fill, stroke, color) = if first_shu && i == 0 {
                (SHU, SHU, ON_SHU)
            } else {
                (CARD, BORDER, INK)
            };
            let _ = write!(
                self.svg,
                "<rect x=\"{left}\" y=\"{y}\" width=\"{w}\" height=\"{height}\" rx=\"{}\" fill=\"{fill}\" stroke=\"{stroke}\" stroke-width=\"2\"/>",
                size * 0.55,
            );
            // The first part at the start: on the left, or on the right in right-to-left text
            let mut at = left + pad;
            let ordered: Vec<&Line> = if self.card.rtl {
                parts.iter().rev().collect()
            } else {
                parts.iter().collect()
            };
            for part in ordered {
                let _ = write!(
                    self.svg,
                    "<text x=\"{at}\" y=\"{}\" font-size=\"{size}\" font-weight=\"600\" fill=\"{color}\">{}</text>",
                    y + height / 2.0 + size * 0.36,
                    part.markup
                );
                at += part.width + space;
            }
            x += w + gap;
        }
    }

    /// Chips in rows that fit the column: one row, a little smaller if that's enough, otherwise
    /// two, smaller when even two don't fit.
    fn chip_rows(&self, texts: &[String], size: f32, families: &[Family]) -> (Vec<Vec<Line>>, f32) {
        let width = |chips: &[Line], size: f32| {
            chips.iter().map(|c| c.width + 1.8 * size).sum::<f32>()
                + 0.6 * size * count(chips.len().saturating_sub(1))
        };
        let line = |size: f32| -> Vec<Line> {
            texts
                .iter()
                .map(|t| self.line(t, families, size, 600))
                .collect()
        };
        for size in [size, size * 0.95, size * 0.9] {
            let chips = line(size);
            if width(&chips, size) <= COLUMN {
                return (vec![chips], size);
            }
        }
        let mut size = size;
        loop {
            let mut rows: Vec<Vec<Line>> = vec![Vec::new()];
            for chip in line(size) {
                let row = rows.last_mut().expect("a row");
                row.push(chip);
                if row.len() > 1 && width(row, size) > COLUMN {
                    let chip = row.pop().expect("a chip");
                    rows.push(vec![chip]);
                }
            }
            if rows.len() <= 2 && rows.iter().all(|r| width(r, size) <= COLUMN) || size < 13.0 {
                return (rows, size);
            }
            size *= 0.95;
        }
    }

    fn site(&mut self, tagline: &str, chips: &[String; 3]) {
        let chain = self.text_chain();
        let (rows, chip_size) = self.chip_rows(chips, 19.0, &chain);
        let (lines, size) = self.wrap(tagline, &chain, 33.0, 700, COLUMN - 20.0, 2);
        let row_height = chip_size * 2.3 + 12.0;
        // Everything above the chips moves up for a second row of chips or tagline
        let lift = row_height * (count(rows.len()) - 1.0) + size * 1.3 * (count(lines.len()) - 2.0);
        let top = |y: f32| y - lift.max(0.0);

        self.hanko(MARGIN, top(96.0), 92.0, "橋");
        let pixel = [Family::Pixel];
        let name = self.fonts.spans("Hashi", &pixel, 400);
        self.text(self.x(MARGIN - 6.0), top(302.0), 124.0, 400, INK, &name);
        let sub = self.fonts.spans("橋をかけろ · HASHIWOKAKERO", &pixel, 400);
        self.text(self.x(MARGIN), top(350.0), 29.0, 400, SHU, &sub);

        let mut y = top(418.0);
        for line in &lines {
            self.line_at(MARGIN, y, size, 700, INK, line, COLUMN);
            y += size * 1.3;
        }
        let mut y = y - size * 0.4;
        for (i, row) in rows.into_iter().enumerate() {
            let row: Vec<Vec<Line>> = row.into_iter().map(|c| vec![c]).collect();
            self.chips(y, &row, chip_size, i == 0);
            y += row_height;
        }
        let domain = self.fonts.spans("hashi.rs", &pixel, 400);
        self.text(self.x(MARGIN), 592.0, 25.0, 400, MUTED, &domain);
    }

    #[expect(clippy::too_many_arguments, reason = "the parts of the daily's header")]
    fn daily(
        &mut self,
        weekday_kanji: &str,
        title: &str,
        date: &str,
        difficulty_kanji: &str,
        difficulty: &str,
        size_text: &str,
        tagline: &str,
    ) {
        self.hanko(MARGIN, 76.0, 104.0, weekday_kanji);
        let pixel = [Family::Pixel];
        let name = self.fonts.spans("Hashi", &pixel, 400);
        self.text(self.x(MARGIN + 134.0), 140.0, 64.0, 400, INK, &name);
        let sub = self.fonts.spans("橋をかけろ · HASHIWOKAKERO", &pixel, 400);
        self.text(self.x(MARGIN + 136.0), 174.0, 20.0, 400, SHU, &sub);

        let chain = self.text_chain();
        let date = self.line(date, &chain, 25.0, 700);
        self.line_at(MARGIN, 262.0, 25.0, 700, MUTED, &date, COLUMN);

        let titles = self.title_chain();
        // One line, a little smaller if that's enough; two only for long titles
        let (lines, size) = [70.0, 64.0, 58.0]
            .into_iter()
            .find_map(|size| {
                let line = self.line(title, &titles, size, 800);
                (line.width <= COLUMN).then(|| (vec![line], size))
            })
            .unwrap_or_else(|| self.wrap(title, &titles, 56.0, 800, COLUMN, 2));
        let mut y = 262.0 + size * 1.15;
        for line in &lines {
            self.line_at(MARGIN, y, size, 800, INK, line, COLUMN);
            y += size * 1.1;
        }

        // The difficulty with its kanji in the pixel font, as on the site
        let chip_size = 21.0;
        let chips = [
            vec![
                self.line(difficulty_kanji, &pixel, chip_size, 400),
                self.line(difficulty, &chain, chip_size, 600),
            ],
            vec![self.line(size_text, &pixel, chip_size, 400)],
        ];
        self.chips(y - chip_size * 0.2, &chips, chip_size, true);

        let (lines, size) = self.wrap(tagline, &chain, 24.0, 700, COLUMN, 1);
        if let Some(line) = lines.first() {
            self.line_at(MARGIN, 548.0, size, 700, MUTED, line, COLUMN);
        }
        let domain = self.fonts.spans("hashi.rs", &pixel, 400);
        self.text(self.x(MARGIN), 592.0, 25.0, 400, MUTED, &domain);
    }
}

/// A text in two at its last comma (`,` `，` `、`), the comma staying with the first part.
fn phrases(text: &str) -> Option<[&str; 2]> {
    let (at, comma) = text
        .char_indices()
        .rfind(|(_, c)| matches!(c, ',' | '，' | '、' | '،'))?;
    let end = at + comma.len_utf8();
    let (first, second) = (text[..end].trim(), text[end..].trim());
    (!first.is_empty() && !second.is_empty()).then_some([first, second])
}

/// Where a text may break: after spaces, and around CJK characters (not before closing
/// punctuation). Each piece keeps its spaces.
pub(super) fn break_points(text: &str) -> Vec<String> {
    fn cjk(c: char) -> bool {
        matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{9fff}' | '\u{f900}'..='\u{faff}')
    }
    fn closing(c: char) -> bool {
        matches!(
            c,
            '、' | '。' | '，' | '．' | '）' | '」' | '』' | '！' | '？' | 'ー' | ',' | '.'
        ) || matches!(
            c,
            'ぁ' | 'ぃ'
                | 'ぅ'
                | 'ぇ'
                | 'ぉ'
                | 'っ'
                | 'ゃ'
                | 'ゅ'
                | 'ょ'
                | 'ァ'
                | 'ィ'
                | 'ゥ'
                | 'ェ'
                | 'ォ'
                | 'ッ'
                | 'ャ'
                | 'ュ'
                | 'ョ'
        )
    }
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut prev: Option<char> = None;
    for c in text.chars() {
        let split = prev.is_some_and(|p| {
            (p == ' ' && c != ' ') || ((cjk(p) || cjk(c) || closing(p)) && !closing(c) && c != ' ')
        });
        if split && !current.is_empty() {
            pieces.push(std::mem::take(&mut current));
        }
        current.push(c);
        prev = Some(c);
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    // A word of one letter stays with the next (Polish, Czech and Russian typography)
    let mut out: Vec<String> = Vec::new();
    for piece in pieces {
        match out.last_mut() {
            Some(last)
                if last.trim().chars().count() == 1
                    && last.ends_with(' ')
                    && last.trim().chars().all(char::is_alphabetic) =>
            {
                last.push_str(&piece);
            }
            _ => out.push(piece),
        }
    }
    out
}

/// The paper: kinari with washi fibres and the seigaiha 青海波 waves fading in at the bottom.
fn background() -> String {
    let s = 64.0_f32;
    let mut rings = String::new();
    // Half rings at the bottom middle of each tile, and of the tiles shifted by half a tile
    for (cx, cy) in [
        (s / 2.0, s / 2.0),
        (s / 2.0, s),
        (0.0, s / 4.0),
        (s, s / 4.0),
        (0.0, s * 0.75),
        (s, s * 0.75),
    ] {
        for r in [0.145, 0.26, 0.375] {
            let r = r * s;
            let _ = write!(
                rings,
                "<path d=\"M{} {cy} A{r} {r} 0 0 1 {} {cy}\"/>",
                cx - r,
                cx + r
            );
        }
    }
    format!(
        "<defs>\
         <pattern id=\"waves\" width=\"{s}\" height=\"{s}\" patternUnits=\"userSpaceOnUse\">\
         <g fill=\"none\" stroke=\"#1f4e79\" stroke-opacity=\"0.11\" stroke-width=\"2\">{rings}</g></pattern>\
         <pattern id=\"fibres\" width=\"23\" height=\"29\" patternUnits=\"userSpaceOnUse\">\
         <circle cx=\"4.6\" cy=\"4.35\" r=\"1.2\" fill=\"#785a32\" fill-opacity=\"0.08\"/></pattern>\
         <pattern id=\"fibres2\" width=\"41\" height=\"43\" patternUnits=\"userSpaceOnUse\">\
         <circle cx=\"16.4\" cy=\"36.5\" r=\"1.4\" fill=\"#785a32\" fill-opacity=\"0.07\"/></pattern>\
         <linearGradient id=\"fade\" x1=\"0\" y1=\"0\" x2=\"0\" y2=\"1\">\
         <stop offset=\"0.45\" stop-color=\"white\" stop-opacity=\"0\"/>\
         <stop offset=\"1\" stop-color=\"white\" stop-opacity=\"1\"/></linearGradient>\
         <mask id=\"low\"><rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"url(#fade)\"/></mask>\
         </defs>\
         <rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"{PAPER}\"/>\
         <rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"url(#fibres)\"/>\
         <rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"url(#fibres2)\"/>\
         <rect width=\"{WIDTH}\" height=\"{HEIGHT}\" fill=\"url(#waves)\" mask=\"url(#low)\"/>"
    )
}

/// The card without its board, as SVG.
pub(super) fn svg(card: &Card, fonts: &Fonts, options: &Options<'static>) -> String {
    let mut layout = Layout {
        fonts,
        options,
        card,
        svg: format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH}\" height=\"{HEIGHT}\" viewBox=\"0 0 {WIDTH} {HEIGHT}\">{}",
            background()
        ),
    };
    match &card.content {
        Content::Site { tagline, chips } => layout.site(tagline, chips),
        Content::Daily {
            weekday_kanji,
            title,
            date,
            difficulty_kanji,
            difficulty,
            size,
            tagline,
        } => layout.daily(
            weekday_kanji,
            title,
            date,
            difficulty_kanji,
            difficulty,
            size,
            tagline,
        ),
    }
    layout.svg.push_str("</svg>");
    layout.svg
}

/// Pixels per art pixel of the board.
const SCALE: u32 = 3;
/// The wooden frame around the board, in image pixels.
const FRAME: u32 = 15;
/// Room for the frame's shadow.
const SHADOW: u32 = 14;

/// The board in its wooden frame, upright, at [`SCALE`].
fn framed_board(board: &BoardArt, options: &Options<'static>) -> Result<Pixmap, String> {
    let [vx, vy, vw, vh] = board.view_box;
    let (w, h) = (vw.unsigned_abs() * SCALE, vh.unsigned_abs() * SCALE);
    let (fw, fh) = (w + 2 * FRAME, h + 2 * FRAME);
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{tw}\" height=\"{th}\">\
         <rect x=\"{sx}\" y=\"{sy}\" width=\"{fw}\" height=\"{fh}\" rx=\"20\" fill=\"#000\" fill-opacity=\"0.13\"/>\
         <rect x=\"{SHADOW}\" y=\"0\" width=\"{fw}\" height=\"{fh}\" rx=\"20\" fill=\"{WOOD_DARK}\"/>\
         <rect x=\"{}\" y=\"4\" width=\"{}\" height=\"{}\" rx=\"17\" fill=\"{WOOD}\"/>\
         <clipPath id=\"og-board\"><rect x=\"{bx}\" y=\"{FRAME}\" width=\"{w}\" height=\"{h}\" rx=\"10\"/></clipPath>\
         <g clip-path=\"url(#og-board)\"><svg x=\"{bx}\" y=\"{FRAME}\" width=\"{w}\" height=\"{h}\" viewBox=\"{vx} {vy} {vw} {vh}\" shape-rendering=\"crispEdges\">{}</svg></g>\
         </svg>",
        SHADOW + 4,
        fw - 8,
        fh - 8,
        board.content,
        tw = fw + SHADOW,
        th = fh + SHADOW,
        sx = 0,
        sy = SHADOW,
        bx = SHADOW + FRAME,
    );
    let tree = usvg::Tree::from_str(&svg, options).map_err(|e| e.to_string())?;
    let size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(size.width(), size.height()).ok_or("empty board")?;
    resvg::render(&tree, Transform::identity(), &mut pixmap.as_mut());
    Ok(pixmap)
}

/// The image as PNG.
pub(super) fn render(card: &Card, fonts: &Fonts) -> Result<Vec<u8>, String> {
    encode(&draw(card, fonts)?)
}

/// The image.
pub(super) fn draw(card: &Card, fonts: &Fonts) -> Result<Pixmap, String> {
    let options = fonts.options();
    let tree =
        usvg::Tree::from_str(&svg(card, fonts, &options), &options).map_err(|e| e.to_string())?;
    let mut pixmap = Pixmap::new(WIDTH, HEIGHT).ok_or("empty image")?;
    resvg::render(&tree, Transform::identity(), &mut pixmap.as_mut());

    // The board, tilted like a photo on the table, running off the edge
    let board = framed_board(&card.board, &options)?;
    let paint = PixmapPaint {
        quality: FilterQuality::Bicubic,
        ..PixmapPaint::default()
    };
    let transform = if card.rtl {
        Transform::from_translate(RIGHT - 668.0, 34.0)
            .pre_concat(Transform::from_rotate(3.5))
            .pre_translate(-px(board.width()), 0.0)
    } else {
        Transform::from_translate(668.0 - px(SHADOW), 34.0).pre_concat(Transform::from_rotate(-3.5))
    };
    pixmap.draw_pixmap(0, 0, board.as_ref(), &paint, transform, None);
    Ok(pixmap)
}

/// PNG with a palette of 256 colours (`NeuQuant`): a third of the size of full colour, which no
/// one sees on a card of a few flat colours. Messengers ignore images over about 300 KB.
pub(super) fn encode(pixmap: &Pixmap) -> Result<Vec<u8>, String> {
    // The card is opaque, its premultiplied pixels are its colours
    let quant = color_quant::NeuQuant::new(10, 256, pixmap.data());
    let indices: Vec<u8> = pixmap
        .data()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| u8::try_from(quant.index_of(p)).unwrap_or(0))
        .collect();
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, pixmap.width(), pixmap.height());
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(quant.color_map_rgb());
    encoder.set_compression(png::Compression::High);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&indices)
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}
