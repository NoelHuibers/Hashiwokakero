//! The board on the preview images: drawn by the site's own sprites (components/sprites.rs),
//! rendered to SVG on the server, in the colours of the site's stylesheet.

use std::fmt::Write as _;

use common::{Board, Puzzle};
use leptos::prelude::*;

use super::card::BoardArt;
use super::fonts::{Family, Fonts};
use crate::components::board::{PAD, span_of};
use crate::components::sprites::{BoardDefs, BridgeSprite, CELL, Decor};

/// The site's stylesheet, for the colours of the pixel art.
const STYLESHEET: &str = include_str!("../../../style/tailwind.css");

/// The lines of the first block of `css` whose selector line is `selector {`.
fn block<'a>(css: &'a str, selector: &str) -> impl Iterator<Item = &'a str> {
    let head = format!("{selector} {{");
    css.lines()
        .skip_while(move |line| line.trim() != head)
        .skip(1)
        .take_while(|line| line.trim() != "}")
}

/// `--name: #hex;` declarations of a block.
fn colours<'a>(lines: impl Iterator<Item = &'a str>) -> Vec<(&'a str, &'a str)> {
    lines
        .filter_map(|line| {
            let (name, value) = line.trim().split_once(':')?;
            let value = value.trim().strip_suffix(';')?;
            (name.starts_with("--") && value.starts_with('#')).then_some((name, value))
        })
        .collect()
}

/// The palette of the day theme as CSS for resvg, which knows no `var()`: `.px-g{fill:#8cc45f}`
/// for every `px-` class and the island numbers, the islands coloured as solved
/// (`.island.done`).
pub(super) fn palette(css: &str) -> String {
    let mut vars = colours(block(css, ":root"));
    // Solved islands set --isl-*, which the px- classes read first
    vars.extend(colours(block(css, ".island.done")));
    let resolve = |expr: &str| {
        expr.split("var(").find_map(|part| {
            let name = part.trim().split([',', ')']).next()?.trim();
            vars.iter().rev().find(|(n, _)| *n == name).map(|(_, v)| *v)
        })
    };
    let fill = |line: &str| {
        let expr = line.split_once("fill:")?.1.split(';').next()?;
        resolve(expr)
    };
    let mut out = String::new();
    for line in css.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(".px-")
            && let Some((name, _)) = rest.split_once(' ')
            && let Some(colour) = fill(line)
        {
            let _ = write!(out, ".px-{name}{{fill:{colour}}}");
        }
    }
    if let Some(colour) = block(css, ".island .num").find_map(fill) {
        let _ = write!(out, ".num{{fill:{colour}}}");
    }
    out
}

/// A solved puzzle as the site draws it, with every island done.
pub(super) fn art(puzzle: &Puzzle, solution: &[u8], fonts: &Fonts) -> BoardArt {
    let board = Board::new(puzzle.clone());
    let width = i32::from(puzzle.width) * CELL + 2 * PAD;
    let height = i32::from(puzzle.height) * CELL + 2 * PAD;
    let sprites = Owner::new().with(|| {
        let bridges = (0..board.edges.len())
            .filter_map(|e| {
                let count = solution.get(e).copied().unwrap_or(0);
                (count > 0).then(|| view! { <BridgeSprite span=span_of(&board, e) count=count /> })
            })
            .collect_view();
        view! {
            <BoardDefs />
            <rect x=-PAD y=-PAD width=width height=height fill="url(#sea)" />
            {bridges}
        }
        .to_html()
    });
    // As the islands layer (components/board/layers.rs) draws them; the numbers in the pixel font
    let mut islands = String::new();
    for island in &puzzle.islands {
        let (x, y) = (island.pos.x, island.pos.y);
        let decor = Decor::for_pos(x, y)
            .id()
            .map(|href| format!("<use href=\"{href}\"/>"))
            .unwrap_or_default();
        let number = fonts.spans(&island.required.to_string(), &[Family::Pixel], 400);
        let _ = write!(
            islands,
            "<g class=\"island done\" transform=\"translate({},{})\">{decor}<use href=\"#isl\"/>\
             <text class=\"num\" x=\"8\" y=\"8.4\" font-size=\"8.6\" text-anchor=\"middle\" dominant-baseline=\"central\">{number}</text></g>",
            i32::from(x) * CELL,
            i32::from(y) * CELL,
        );
    }
    BoardArt {
        // Leptos marks the places of dynamic children with `<!>`, which isn't XML
        content: format!(
            "<style>{}</style>{}{islands}",
            palette(STYLESHEET),
            sprites.replace("<!>", "")
        ),
        view_box: [-PAD, -PAD, width, height],
    }
}
