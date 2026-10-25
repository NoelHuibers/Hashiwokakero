//! Social preview images (Open Graph, `og:image`): one per language, one of the day's daily.
//!
//! Shared links get a card in the language of the page; shared daily results link the home
//! page, so they show today's card.
//!
//! Drawn on the server when first asked for and kept in memory: an SVG of the card, rasterized
//! with resvg (see `card.rs`), with the site's own board sprites and fonts. The daily's card
//! shows its number, date, difficulty and size, never its puzzle: its board is decorative, made
//! by the generator from a public seed.
//!
//! - `/og/<lang>.png`: the card of a language (`seo::og_image`)
//! - `/og/daily/<date>/<lang>.png`: the card of a day's daily (`seo::og_daily_image`), the date
//!   in the URL so link previews cached yesterday don't show yesterday's
//! - `/og.png`: the English card, where links shared before these existed point

mod board;
mod card;
mod fonts;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use axum::body::Bytes;
use axum::extract::Path;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use common::{Difficulty, GenerateParams};
use leptos_i18n::Locale as _;

use self::card::{BoardArt, Card, Content};
use self::fonts::{Family, Fonts};
use super::coalesce::Flights;
use crate::i18n::{Locale, td_string};
use crate::lang::{date_long, difficulty_name};
use crate::model::{
    LAUNCH_DAY, WEEKDAY_KANJI, daily_difficulty, daily_number, parse_date, weekday,
};

/// The images of a language never change between deploys.
const SITE_CACHE: &str = "public, max-age=86400";
/// A day's image never changes.
const DAILY_CACHE: &str = "public, max-age=31536000, immutable";

/// A preview image.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Image {
    /// The site, for every page of a language
    Site(Locale),
    /// The daily of a day, for the home page
    Daily(Locale, i64),
}

static FONTS: LazyLock<Fonts> = LazyLock::new(Fonts::load);

/// The language's text and heading fonts, as the site's (`--app-font`, `--display-font` in
/// style/tailwind.css).
fn families(locale: Locale) -> (Family, Family) {
    let text = match locale {
        Locale::ko => Family::NotoSansKr,
        Locale::zh => Family::NotoSansSc,
        Locale::zh_Hant => Family::NotoSansTc,
        Locale::ar => Family::NotoSansArabic,
        Locale::hi => Family::NotoSansDevanagari,
        Locale::th => Family::NotoSansThai,
        Locale::vi => Family::BeVietnamPro,
        _ => Family::Murecho,
    };
    // Kaisei Decol, as the site's headings, in Latin and Japanese. Its Cyrillic letters are as
    // wide as kanji, a title in it looks letterspaced.
    let title = match locale {
        Locale::ru | Locale::uk => Family::Murecho,
        _ if text == Family::Murecho => Family::KaiseiDecol,
        _ => text,
    };
    (text, title)
}

/// A decorative board: solved, made by the generator from a public seed. The daily's are 12×10,
/// a size no daily has, so none is ever the puzzle of a day.
fn decoration(difficulty: Difficulty, seed: u64) -> BoardArt {
    let generated = backend::api::generate(&GenerateParams {
        width: 12,
        height: 10,
        difficulty,
        seed,
    });
    board::art(&generated.puzzle, &generated.solution, &FONTS)
}

/// The board of the language cards.
static SITE_BOARD: LazyLock<BoardArt> =
    LazyLock::new(|| decoration(Difficulty::Medium, 0x4861_7368_6921));

/// The language of an image.
fn locale(image: Image) -> Locale {
    match image {
        Image::Site(locale) | Image::Daily(locale, _) => locale,
    }
}

/// What a card says.
fn content(image: Image) -> Content {
    match image {
        Image::Site(locale) => Content::Site {
            tagline: td_string!(locale, common.og.tagline).to_string(),
            chips: [
                td_string!(locale, common.og.free).to_string(),
                td_string!(locale, common.og.daily).to_string(),
                td_string!(locale, common.og.unique).to_string(),
            ],
        },
        Image::Daily(locale, day) => {
            let difficulty = daily_difficulty(day);
            let (w, h) = difficulty.size();
            Content::Daily {
                weekday_kanji: WEEKDAY_KANJI[weekday(day)],
                title: td_string!(locale, daily.title, number = daily_number(day)),
                date: date_long(locale, day),
                difficulty_kanji: difficulty.kanji(),
                difficulty: difficulty_name(locale, difficulty),
                size: format!("{w}×{h}"),
                tagline: td_string!(locale, common.og.daily).to_string(),
            }
        }
    }
}

/// The card of an image, with its board.
fn card(image: Image, board: BoardArt) -> Card {
    let locale = locale(image);
    let (text_font, title_font) = families(locale);
    Card {
        content: content(image),
        rtl: locale == Locale::ar,
        text_font,
        title_font,
        board,
    }
}

/// The image as PNG.
pub fn render(image: Image) -> Result<Vec<u8>, String> {
    let board = match image {
        Image::Site(_) => SITE_BOARD.clone(),
        // Another board every day
        Image::Daily(_, day) => decoration(daily_difficulty(day), day.unsigned_abs()),
    };
    card::render(&card(image, board), &FONTS)
}

/// Rendered images. The languages' stay, the dailies' only for today and yesterday.
static CACHE: LazyLock<Mutex<HashMap<Image, Bytes>>> = LazyLock::new(Mutex::default);

/// Images being rendered: the first requests of one (the previews of a shared link, fetched by
/// several apps at once) wait for one rendering. At most one key per language and day.
static RENDERING: LazyLock<Flights<Image, Result<Bytes, StatusCode>>> =
    LazyLock::new(|| Flights::new(64));

async fn cached(image: Image) -> Result<Bytes, StatusCode> {
    cached_with(&CACHE, &RENDERING, image, render).await
}

/// [`cached`] with its own cache and renderings, made by `render`.
async fn cached_with(
    cache: &Mutex<HashMap<Image, Bytes>>,
    rendering: &Flights<Image, Result<Bytes, StatusCode>>,
    image: Image,
    render: impl FnOnce(Image) -> Result<Vec<u8>, String> + Send + 'static,
) -> Result<Bytes, StatusCode> {
    let kept = || cache.lock().ok().and_then(|c| c.get(&image).cloned());
    if let Some(png) = kept() {
        return Ok(png);
    }
    rendering
        .run(image, || async move {
            // A rendering may have ended since
            if let Some(png) = kept() {
                return Ok(png);
            }
            let png = Bytes::from(
                super::run_blocking(move || render(image))
                    .await
                    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
            );
            // Before the rendering ends: later requests find it here
            if let Ok(mut cache) = cache.lock() {
                let today = super::daily::today();
                cache.retain(|i, _| !matches!(i, Image::Daily(_, day) if *day < today - 1));
                cache.insert(image, png.clone());
            }
            Ok(png)
        })
        .await
}

/// The language of `<lang>.png`.
fn locale_of(file: &str) -> Option<Locale> {
    let lang = file.strip_suffix(".png")?;
    Locale::get_all()
        .iter()
        .copied()
        .find(|l| l.as_str() == lang)
}

/// The image of `<date>/<lang>.png` (`2026-10-20/de.png`), for the days with a daily up to
/// tomorrow (link previews may be made around midnight in another time zone).
fn daily_of(date: &str, file: &str) -> Option<Image> {
    let day = parse_date(date)?;
    let today = super::daily::today();
    (LAUNCH_DAY..=today + 1)
        .contains(&day)
        .then_some(Image::Daily(locale_of(file)?, day))
}

async fn respond(image: Option<Image>, cache_control: &'static str) -> Response {
    let Some(image) = image else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match cached(image).await {
        Ok(png) => (
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, cache_control),
            ],
            png,
        )
            .into_response(),
        Err(status) => status.into_response(),
    }
}

/// `GET /og/<lang>.png`
pub async fn site_image(Path(file): Path<String>) -> Response {
    respond(locale_of(&file).map(Image::Site), SITE_CACHE).await
}

/// `GET /og/daily/<date>/<lang>.png`
pub async fn daily_image(Path((date, file)): Path<(String, String)>) -> Response {
    respond(daily_of(&date, &file), DAILY_CACHE).await
}

/// `GET /og.png`, where links shared before the images were made per language point.
pub async fn legacy_image() -> Response {
    respond(Some(Image::Site(Locale::en)), SITE_CACHE).await
}
