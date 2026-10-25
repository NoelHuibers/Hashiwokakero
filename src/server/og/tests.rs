use leptos_i18n::Locale as _;
use resvg::usvg::{self, Node};

use super::card::{self, HEIGHT, WIDTH};
use super::*;
use crate::model::{date_string, day_of};

/// Width and height in the header of a PNG.
fn png_size(png: &[u8]) -> (u32, u32) {
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
    assert_eq!(&png[12..16], b"IHDR");
    let word = |at: usize| u32::from_be_bytes(png[at..at + 4].try_into().unwrap());
    (word(16), word(20))
}

/// Text of the glyphs a font didn't have (drawn as an empty box), in every `<text>` of `svg`.
fn missing_glyphs(svg: &str) -> Vec<String> {
    fn walk(group: &usvg::Group, out: &mut Vec<String>) {
        for node in group.children() {
            match node {
                Node::Group(g) => walk(g, out),
                Node::Text(text) => {
                    for span in text.layouted() {
                        out.extend(
                            span.positioned_glyphs
                                .iter()
                                .filter(|g| g.id.0 == 0)
                                .filter(|g| !g.text.chars().all(|c| c.is_whitespace() || c.is_control() || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}')))
                                .map(|g| g.text.clone()),
                        );
                    }
                }
                _ => {}
            }
        }
    }
    let tree = usvg::Tree::from_str(svg, &FONTS.options()).unwrap();
    let mut out = Vec::new();
    walk(tree.root(), &mut out);
    out
}

/// A card without its board, which the glyph tests don't need.
fn text_card(image: Image) -> String {
    let board = BoardArt {
        content: String::new(),
        view_box: [0, 0, 16, 16],
    };
    card::svg(&card(image, board), &FONTS, &FONTS.options())
}

#[test]
fn images_are_pngs_of_1200_by_630_small_enough_for_whatsapp() {
    for image in [
        Image::Site(Locale::en),
        Image::Site(Locale::ar),
        Image::Daily(Locale::ja, LAUNCH_DAY + 3),
    ] {
        let png = render(image).unwrap();
        assert_eq!(png_size(&png), (WIDTH, HEIGHT), "{image:?}");
        // WhatsApp shows no preview for images over about 300 KB
        assert!(png.len() < 300 * 1024, "{image:?}: {} bytes", png.len());
    }
}

/// Every language's cards in its fonts, without a single empty box. When this fails after a
/// translation changed, `pnpm run og-fonts` cuts the fonts to the new texts.
#[test]
fn every_language_has_the_glyphs_of_its_cards() {
    for &locale in Locale::get_all() {
        let missing = missing_glyphs(&text_card(Image::Site(locale)));
        assert!(missing.is_empty(), "{locale:?}: {missing:?}");
        let missing = missing_glyphs(&text_card(Image::Daily(locale, LAUNCH_DAY + 3)));
        assert!(missing.is_empty(), "{locale:?} daily: {missing:?}");
    }
}

/// The daily's texts of every weekday, month and difficulty, and the digits, in every language.
#[test]
fn every_date_has_its_glyphs() {
    // Every 31 days: twelve months, and every weekday within seven of them
    let days: Vec<i64> = (0..12).map(|k| LAUNCH_DAY + 31 * k).collect();
    for &locale in Locale::get_all() {
        let (text_font, title_font) = families(locale);
        for &day in &days {
            let Content::Daily {
                weekday_kanji,
                title,
                date,
                difficulty_kanji,
                difficulty,
                size,
                tagline,
            } = content(Image::Daily(locale, day))
            else {
                unreachable!()
            };
            let text = [text_font, Family::Murecho];
            let pixel = [Family::Pixel];
            for (s, families, weight) in [
                (date.as_str(), &text[..], 700),
                (difficulty.as_str(), &text[..], 600),
                (tagline.as_str(), &text[..], 700),
                (title.as_str(), &[title_font, Family::Murecho][..], 800),
                (weekday_kanji, &pixel[..], 400),
                (difficulty_kanji, &pixel[..], 400),
                (size.as_str(), &pixel[..], 400),
            ] {
                let missing = FONTS.missing(s, families, weight);
                assert!(missing.is_empty(), "{locale:?} {s}: {missing:?}");
            }
        }
    }
}

/// Every glyph of `group` with its x position.
fn glyph_positions(group: &usvg::Group, out: &mut Vec<(String, f32)>) {
    for node in group.children() {
        match node {
            Node::Group(g) => glyph_positions(g, out),
            Node::Text(text) => {
                for span in text.layouted() {
                    for g in &span.positioned_glyphs {
                        out.push((g.text.clone(), g.transform().tx));
                    }
                }
            }
            _ => {}
        }
    }
}

/// Right-to-left text in an embedding, so numbers stand where an Arabic reader expects them:
/// the daily's number after its name, to the left of it (as browsers show the site's header).
#[test]
fn arabic_is_shaped_right_to_left() {
    let svg = text_card(Image::Daily(Locale::ar, LAUNCH_DAY + 3));
    assert!(svg.contains('\u{202b}'), "no embedding");
    let tree = usvg::Tree::from_str(&svg, &FONTS.options()).unwrap();
    // The title: its "#4" left of the Arabic words
    let title = td_string!(Locale::ar, daily.title, number = 4);
    let mut glyphs = Vec::new();
    glyph_positions(tree.root(), &mut glyphs);
    let x = |s: &str| {
        glyphs
            .iter()
            .filter(|(t, _)| t == s)
            .map(|(_, x)| *x)
            .next()
            .unwrap_or_else(|| panic!("no {s} in {title}"))
    };
    assert!(x("4") < x("ي"), "{glyphs:?}");
}

#[test]
fn texts_break_at_spaces_and_between_cjk_characters() {
    assert_eq!(
        card::break_points("The Japanese bridges"),
        ["The ", "Japanese ", "bridges"]
    );
    // Never before closing punctuation or small kana
    assert_eq!(
        card::break_points("橋のパズルを、ドット絵で"),
        [
            "橋", "の", "パ", "ズ", "ル", "を、", "ドッ", "ト", "絵", "で"
        ]
    );
    assert_eq!(card::break_points("Hashi 橋"), ["Hashi ", "橋"]);
    // A word of one letter stays with the next one
    assert_eq!(
        card::break_points("z mostami, w pixel arcie"),
        ["z mostami, ", "w pixel ", "arcie"]
    );
}

#[test]
#[expect(
    clippy::literal_string_with_formatting_args,
    reason = "CSS rules, not format strings"
)]
fn the_board_is_the_sites_in_its_colours() {
    let css = board::palette(include_str!("../../../style/tailwind.css"));
    // Solved islands: a pale green plate, dark green numbers
    assert!(css.contains(".px-P{fill:#e5f2c9}"), "{css}");
    assert!(css.contains(".num{fill:#3e6b12}"), "{css}");
    assert!(css.contains(".px-sea{fill:#59acd0}"), "{css}");
    assert!(css.contains(".px-o{fill:#2b2420}"), "{css}");

    let art = &*SITE_BOARD;
    assert!(!art.content.contains("<!>"));
    // Every colour the sprites use is in the palette
    for class in art.content.split("class=\"").skip(1) {
        let class = class.split('"').next().unwrap();
        for name in class.split(' ').filter(|c| c.starts_with("px-")) {
            assert!(css.contains(&format!(".{name}{{")), "no colour for {name}");
        }
    }
    // Every island, with its number in the pixel font
    let islands = art.content.matches("<use href=\"#isl\"/>").count();
    assert!(islands > 10, "{islands} islands");
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{}\">{}</svg>",
        art.view_box.map(|v| v.to_string()).join(" "),
        art.content
    );
    assert_eq!(missing_glyphs(&svg), Vec::<String>::new());
}

#[test]
fn no_decorative_board_is_a_daily() {
    // The decorations are 12×10, no daily has that size
    assert!(Difficulty::ALL.iter().all(|d| d.size() != (12, 10)));
    assert_eq!(SITE_BOARD.view_box[2..], [12 * 16 + 12, 10 * 16 + 12]);
}

#[test]
fn image_urls_name_language_and_day() {
    assert_eq!(locale_of("de.png"), Some(Locale::de));
    assert_eq!(locale_of("zh-Hant.png"), Some(Locale::zh_Hant));
    for file in ["de", "xx.png", "de.jpg", ".png", "en.png.png"] {
        assert_eq!(locale_of(file), None, "{file}");
    }

    let today = super::super::daily::today();
    let date = |day| date_string(day);
    assert_eq!(
        daily_of(&date(today), "ja.png"),
        Some(Image::Daily(Locale::ja, today))
    );
    assert_eq!(
        daily_of(&date(LAUNCH_DAY), "en.png"),
        Some(Image::Daily(Locale::en, LAUNCH_DAY))
    );
    // Tomorrow's, for previews made before midnight in UTC, but no further
    assert!(daily_of(&date(today + 1), "en.png").is_some());
    assert_eq!(daily_of(&date(today + 2), "en.png"), None);
    assert_eq!(daily_of(&date(LAUNCH_DAY - 1), "en.png"), None);
    assert_eq!(daily_of("2026-13-01", "en.png"), None);
    assert_eq!(daily_of(&date(today), "xx.png"), None);
    assert_eq!(day_of(crate::server::now_ms()), today);
}

/// The previews of a shared link, asked for at once: one rendering, the same PNG for all, then
/// kept. Another image renders on its own.
#[tokio::test]
async fn concurrent_requests_of_an_image_share_one_rendering() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    const REQUESTS: usize = 6;
    static RENDERS: AtomicUsize = AtomicUsize::new(0);
    static RENDERING: LazyLock<Flights<Image, Result<Bytes, StatusCode>>> =
        LazyLock::new(|| Flights::new(4));
    let cache = Mutex::default();
    let image = Image::Daily(Locale::de, LAUNCH_DAY);
    let render = move |rendered: Image| {
        RENDERS.fetch_add(1, Ordering::SeqCst);
        // Done once all the other requests wait for it
        for _ in 0..2000 {
            if RENDERING.followers(&image) == REQUESTS - 1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        Ok(format!("{rendered:?}").into_bytes())
    };
    let pngs = futures::future::join_all(
        (0..REQUESTS).map(|_| cached_with(&cache, &RENDERING, image, render)),
    )
    .await;
    assert_eq!(RENDERS.load(Ordering::SeqCst), 1);
    let expected = Bytes::from(format!("{image:?}"));
    assert!(pngs.iter().all(|png| png.as_ref() == Ok(&expected)));
    assert_eq!(cache.lock().unwrap().get(&image), Some(&expected));
    assert_eq!(RENDERING.in_flight(), 0);
    // Kept: no rendering. Another language renders.
    cached_with(&cache, &RENDERING, image, render)
        .await
        .unwrap();
    assert_eq!(RENDERS.load(Ordering::SeqCst), 1);
    let other = Image::Daily(Locale::ja, LAUNCH_DAY);
    let png = cached_with(&cache, &RENDERING, other, render).await;
    assert_eq!(png, Ok(Bytes::from(format!("{other:?}"))));
    assert_eq!(RENDERS.load(Ordering::SeqCst), 2);
}

/// Writes every card to `$OG_SAMPLES` (a directory), to look at them:
/// `OG_SAMPLES=~/og cargo test --no-default-features --features ssr --lib og::tests::samples -- --ignored`
#[test]
#[ignore = "writes images to look at"]
fn samples() {
    let dir = std::path::PathBuf::from(std::env::var("OG_SAMPLES").expect("OG_SAMPLES"));
    std::fs::create_dir_all(&dir).unwrap();
    let today = super::super::daily::today();
    let art = &*SITE_BOARD;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{}\">{}</svg>",
        art.view_box.map(|v| v.to_string()).join(" "),
        art.content
    );
    std::fs::write(dir.join("board.svg"), svg).unwrap();
    for &locale in Locale::get_all() {
        let png = render(Image::Site(locale)).unwrap();
        std::fs::write(dir.join(format!("{}.png", locale.as_str())), png).unwrap();
    }
    for locale in [
        Locale::en,
        Locale::de,
        Locale::ja,
        Locale::ar,
        Locale::th,
        Locale::hi,
        Locale::zh_Hant,
    ] {
        for day in [today, today + 4] {
            let png = render(Image::Daily(locale, day)).unwrap();
            let name = format!("daily-{}-{}.png", date_string(day), locale.as_str());
            std::fs::write(dir.join(name), png).unwrap();
        }
    }
}
