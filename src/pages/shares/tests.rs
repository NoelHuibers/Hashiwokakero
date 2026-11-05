//! The page of a shared puzzle as the server renders it.

use super::*;
use crate::model::{BLOCKED_NAME, SharedBy};

fn puzzle() -> Puzzle {
    Puzzle::from_grid(&[vec![2, 0, 3], vec![0, 0, 0], vec![0, 0, 1]])
}

fn entry(rank: i64, user_id: i64, name: &str, time_ms: i64, hints: i64) -> SharedEntry {
    SharedEntry {
        rank,
        user_id,
        name: name.into(),
        guest: false,
        stamp: None,
        time_ms,
        moves: 3,
        hints,
        score_ms: crate::model::share_score(time_ms, hints),
    }
}

/// aiko's puzzle, solved by her (02:13) and by bram (01:40 and a hint: 02:10), seen by `me`.
fn page(me: Option<i64>) -> SharedPuzzlePage {
    let aiko = entry(2, 1, "aiko", 133_000, 0);
    let bram = entry(1, 2, "bram", 100_000, 1);
    SharedPuzzlePage {
        code: "k7m2xq9a".into(),
        puzzle: puzzle(),
        sharer: Some(SharedBy {
            user_id: 1,
            name: "aiko".into(),
            guest: false,
            stamp: None,
            result: Some(aiko.clone()),
        }),
        board: vec![bram.clone(), aiko.clone()],
        mine: me.and_then(|me| {
            [&aiko, &bram]
                .into_iter()
                .find(|e| e.user_id == me)
                .cloned()
        }),
        solvers: 2,
        created_at: 0,
    }
}

fn render(page: SharedPuzzlePage) -> String {
    _ = any_spawner::Executor::init_tokio();
    let owner = Owner::new_root(None);
    let html = owner.with(|| {
        crate::app::provide_server_i18n();
        use_i18n().set_locale(Locale::en);
        provide_context(leptos_router::location::RequestUrl::new("/p/k7m2xq9a"));
        provide_context(state::Rivals::new());
        provide_context(Settings::new());
        view! {
            <leptos_router::components::Router>
                <Playing initial=page />
            </leptos_router::components::Router>
        }
        .to_html()
    });
    drop(owner);
    html
}

#[tokio::test]
async fn the_page_shows_the_sharer_the_time_to_beat_and_the_board() {
    let html = render(page(None));
    for text in [
        "aiko's puzzle",
        "Shared by aiko",
        "3×3 · 3 islands",
        "Time to beat",
        "02:13",
        "Everyone who solved it",
        "Solvers: 2",
        "02:10",
        "1h",
        "data-sharer",
        "Hint",
        "Check",
    ] {
        assert!(html.contains(text), "{text}: {html}");
    }
    // bram (01:40 + 30 s) first: rows in order
    assert!(html.find("data-user=\"2\"").unwrap() < html.find("data-user=\"1\"").unwrap());
    assert!(!html.contains("shared-result"));
}

#[test]
fn names_follow_blocks_and_deletions() {
    let mut p = page(None);
    assert_eq!(shared_title(Locale::en, &p), "aiko's puzzle");
    assert_eq!(byline(Locale::en, &p), "Shared by aiko");
    let (title, description) = preview(Locale::en, &p);
    assert_eq!(title, "aiko's Hashi puzzle");
    assert!(description.contains("in 02:13"), "{description}");
    // Blocked: no name anywhere
    p.sharer.as_mut().unwrap().name = BLOCKED_NAME.into();
    assert_eq!(shared_title(Locale::en, &p), "A shared puzzle");
    assert_eq!(byline(Locale::en, &p), "Shared by a blocked player");
    assert!(!preview(Locale::en, &p).1.contains(BLOCKED_NAME));
    // Workshop, no time
    p.sharer.as_mut().unwrap().name = "aiko".into();
    p.sharer.as_mut().unwrap().result = None;
    assert!(preview(Locale::en, &p).1.contains("built this 3×3"));
    // Gone
    p.sharer = None;
    assert_eq!(byline(Locale::en, &p), "Shared by a former player");
    assert_eq!(preview(Locale::en, &p).0, "A shared Hashi puzzle");
}
