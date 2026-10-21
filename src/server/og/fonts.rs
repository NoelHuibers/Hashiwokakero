//! The fonts of the preview images: the site's fonts, cut down to the characters the images draw
//! by `scripts/og-fonts.mjs` (run it again when their texts change).
//!
//! The font packages split every font into slices by script, so a font is a collection of faces
//! here, one per slice. resvg replaces the glyphs a font lacks with another font's only when both
//! give the same number of glyphs, which Arabic, Devanagari or Thai next to digits never do. So a
//! text is drawn in spans, each with a face that has all of its characters ([`Fonts::spans`]),
//! named by the index of the face (`f12`), which [`Fonts::options`] resolves.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::Arc;

use resvg::usvg::fontdb::{Database, ID, Source};
use resvg::usvg::{FontFamily, FontResolver, Options};
use skrifa::MetadataProvider as _;

/// A font of the images, one file in `fonts/` each.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Family {
    /// `DotGothic16`, the pixel font: the name, numbers, kanji
    Pixel,
    /// Kaisei Decol, the headings of the languages Murecho covers
    KaiseiDecol,
    /// The text font of Latin, Cyrillic and Japanese, everyone's fallback
    Murecho,
    BeVietnamPro,
    NotoSansKr,
    NotoSansSc,
    NotoSansTc,
    NotoSansArabic,
    NotoSansDevanagari,
    NotoSansThai,
}

/// The files `scripts/og-fonts.mjs` writes, TrueType collections.
const FILES: [(Family, &[u8]); 10] = [
    (Family::Pixel, include_bytes!("fonts/dotgothic16.ttc")),
    (
        Family::KaiseiDecol,
        include_bytes!("fonts/kaisei-decol.ttc"),
    ),
    (Family::Murecho, include_bytes!("fonts/murecho.ttc")),
    (
        Family::BeVietnamPro,
        include_bytes!("fonts/be-vietnam-pro.ttc"),
    ),
    (Family::NotoSansKr, include_bytes!("fonts/noto-sans-kr.ttc")),
    (Family::NotoSansSc, include_bytes!("fonts/noto-sans-sc.ttc")),
    (Family::NotoSansTc, include_bytes!("fonts/noto-sans-tc.ttc")),
    (
        Family::NotoSansArabic,
        include_bytes!("fonts/noto-sans-arabic.ttc"),
    ),
    (
        Family::NotoSansDevanagari,
        include_bytes!("fonts/noto-sans-devanagari.ttc"),
    ),
    (
        Family::NotoSansThai,
        include_bytes!("fonts/noto-sans-thai.ttc"),
    ),
];

struct Face {
    family: Family,
    /// The weight of a static font; variable ones report their default and get any weight
    weight: u16,
    id: ID,
    chars: HashSet<char>,
}

/// All faces, loaded once.
pub(super) struct Fonts {
    db: Arc<Database>,
    faces: Vec<Face>,
}

impl std::fmt::Debug for Fonts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Fonts({} faces)", self.faces.len())
    }
}

/// Characters that take the face of the text around them: bidi controls and joiners.
fn is_control(c: char) -> bool {
    matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

impl Fonts {
    pub(super) fn load() -> Self {
        let mut db = Database::new();
        let mut faces = Vec::new();
        for (family, bytes) in FILES {
            let ids = db.load_font_source(Source::Binary(Arc::new(bytes)));
            for (index, id) in ids.into_iter().enumerate() {
                let font = u32::try_from(index)
                    .ok()
                    .and_then(|i| skrifa::FontRef::from_index(bytes, i).ok());
                let chars = font
                    .map(|f| {
                        f.charmap()
                            .mappings()
                            .filter_map(|(c, _)| char::from_u32(c))
                            .collect()
                    })
                    .unwrap_or_default();
                let weight = db.face(id).map_or(400, |f| f.weight.0);
                faces.push(Face {
                    family,
                    weight,
                    id,
                    chars,
                });
            }
        }
        Self {
            db: Arc::new(db),
            faces,
        }
    }

    /// Options for parsing an SVG that names its fonts by face (`f12`, see [`Self::spans`]).
    pub(super) fn options(&self) -> Options<'static> {
        let ids: Arc<HashMap<String, ID>> = Arc::new(
            self.faces
                .iter()
                .enumerate()
                .map(|(i, f)| (format!("f{i}"), f.id))
                .collect(),
        );
        Options {
            fontdb: self.db.clone(),
            font_resolver: FontResolver {
                select_font: Box::new(move |font, _| {
                    font.families().iter().find_map(|family| match family {
                        FontFamily::Named(name) => ids.get(name).copied(),
                        _ => None,
                    })
                }),
                // Every span has a face with all of its characters
                select_fallback: Box::new(|_, _, _| None),
            },
            ..Options::default()
        }
    }

    /// The face for a character: the first family of `families` that has it, of that family the
    /// face closest to `weight`.
    fn face(&self, c: char, families: &[Family], weight: u16) -> Option<usize> {
        families.iter().find_map(|family| {
            self.faces
                .iter()
                .enumerate()
                .filter(|(_, f)| f.family == *family && f.chars.contains(&c))
                .min_by_key(|(_, f)| f.weight.abs_diff(weight))
                .map(|(i, _)| i)
        })
    }

    /// The characters of `text` none of the `families` has.
    #[cfg(test)]
    pub(super) fn missing(&self, text: &str, families: &[Family], weight: u16) -> Vec<char> {
        text.chars()
            .filter(|c| !is_control(*c) && self.face(*c, families, weight).is_none())
            .collect()
    }

    /// `text` as `<tspan>`s, each in one face (`font-family="f12"`) that has all its characters:
    /// the first of `families` that has a character. Spaces and bidi controls stay in the face
    /// of the text before them.
    pub(super) fn spans(&self, text: &str, families: &[Family], weight: u16) -> String {
        // Text that starts with spaces or controls: they go with the first character that
        // decides. A span of their own would be shaped in another face, and resvg puts the
        // glyphs of the spans of a text together by byte position, which loses letters of
        // joined scripts when the first span's face doesn't have them.
        let first = text
            .chars()
            .filter(|c| !is_control(*c) && !c.is_whitespace())
            .find_map(|c| self.face(c, families, weight));
        let fallback = first
            .or_else(|| self.face('a', families, weight))
            .unwrap_or_default();
        let mut runs: Vec<(usize, String)> = Vec::new();
        for c in text.chars() {
            let current = runs.last().map(|(face, _)| *face);
            let keep = current.filter(|&f| {
                is_control(c) || (c.is_whitespace() && self.faces[f].chars.contains(&c))
            });
            let face = keep
                .or_else(|| {
                    if is_control(c) {
                        None
                    } else {
                        self.face(c, families, weight)
                    }
                })
                .or(current)
                .unwrap_or(fallback);
            match runs.last_mut() {
                Some((f, run)) if *f == face => run.push(c),
                _ => runs.push((face, c.to_string())),
            }
        }
        let mut out = String::new();
        for (face, run) in runs {
            let _ = write!(
                out,
                "<tspan font-family=\"f{face}\">{}</tspan>",
                escape(&run)
            );
        }
        out
    }
}

/// Text for XML.
pub(super) fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}
