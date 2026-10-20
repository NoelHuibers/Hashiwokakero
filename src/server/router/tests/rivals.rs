//! Rivals over HTTP: a visitor marks one and becomes a guest, the spoiler rule, and the rivals
//! following a guest into their account and going with a deleted one.

use super::*;
use crate::api::{
    CurrentUser, DailyReveal, DailyToday, DeleteAccount, LoadRivalsBoard, Login, MyRivals,
    RivalRecord, SetRival, StartDaily, TodaysRivals,
};
use crate::model::HeadToHead;

fn mark(player: i64, on: bool) -> SetRival {
    SetRival { player, on }
}

#[tokio::test]
async fn rivals_over_http() {
    let server = Server::start().await;
    let conn = conn().await;
    let keiko = Browser::signed_up(&server, "rival_keiko").await;
    let keiko_id = user_id(&conn, "rival_keiko").await;

    // Without a session: no rivals, nothing to compare, and no guest made for asking
    let visitor = server.browser();
    assert_eq!(visitor.call(MyRivals {}).await.unwrap(), Vec::<i64>::new());
    assert_eq!(visitor.call(TodaysRivals {}).await.unwrap(), None);
    assert_eq!(visitor.call(LoadRivalsBoard {}).await.unwrap(), None);
    assert_eq!(
        visitor
            .call(RivalRecord { player: keiko_id })
            .await
            .unwrap(),
        None
    );
    assert!(visitor.session().is_none());

    // Marking one makes the visitor a guest, like starting the daily
    assert_eq!(
        visitor.call(mark(keiko_id, true)).await.unwrap(),
        [keiko_id]
    );
    let guest = visitor.call(CurrentUser {}).await.unwrap().unwrap();
    assert!(guest.guest);
    assert_eq!(visitor.call(MyRivals {}).await.unwrap(), [keiko_id]);
    assert_eq!(
        api_error(visitor.call(mark(guest.id, true)).await),
        ApiError::RivalSelf
    );
    assert_eq!(
        api_error(visitor.call(mark(i64::MAX, true)).await),
        ApiError::NoSuchPlayer
    );
    assert_eq!(
        visitor
            .call(RivalRecord { player: keiko_id })
            .await
            .unwrap(),
        Some(HeadToHead::default())
    );
    let board = visitor.call(LoadRivalsBoard {}).await.unwrap().unwrap();
    let rows: Vec<(i64, bool)> = board.rows.iter().map(|r| (r.entry.user_id, r.me)).collect();
    assert!(rows.contains(&(guest.id, true)) && rows.contains(&(keiko_id, false)));

    // Nothing about today before the guest finished: not while it runs, only once given up
    visitor.call(StartDaily { device: None }).await.unwrap();
    assert_eq!(visitor.call(TodaysRivals {}).await.unwrap(), None);
    let day = visitor.call(DailyToday {}).await.unwrap().day;
    visitor.call(DailyReveal { day }).await.unwrap();
    let today = visitor.call(TodaysRivals {}).await.unwrap().unwrap();
    assert_eq!(today.mine, None);
    assert_eq!(today.rivals.len(), 1);
    assert_eq!(
        (today.rivals[0].user_id, today.rivals[0].name.as_str()),
        (keiko_id, "rival_keiko")
    );

    // keiko marks the guest back, unbeknownst to them
    assert_eq!(keiko.call(mark(guest.id, true)).await.unwrap(), [guest.id]);

    // The guest signs in to their account: their rivals come along, and keiko's rival is the
    // account now
    let _ = Browser::signed_up(&server, "rival_rita").await;
    let rita = visitor
        .call(Login {
            username: "rival_rita".into(),
            password: PASSWORD.into(),
        })
        .await
        .unwrap();
    assert_eq!(visitor.call(MyRivals {}).await.unwrap(), [keiko_id]);
    assert_eq!(keiko.call(MyRivals {}).await.unwrap(), [rita.id]);

    // Unmarking
    assert_eq!(
        visitor.call(mark(keiko_id, false)).await.unwrap(),
        Vec::<i64>::new()
    );
    visitor.call(mark(keiko_id, true)).await.unwrap();

    // Deleting the account takes its list and its place on keiko's
    visitor
        .call(DeleteAccount {
            password: Some(PASSWORD.into()),
        })
        .await
        .unwrap();
    assert_eq!(keiko.call(MyRivals {}).await.unwrap(), Vec::<i64>::new());
    let mut left = conn
        .query(
            "SELECT COUNT(*) FROM rivals WHERE user_id = ?1 OR rival_id = ?1",
            params![rita.id],
        )
        .await
        .unwrap();
    assert_eq!(
        left.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        0
    );
}
