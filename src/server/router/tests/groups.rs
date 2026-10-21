//! Groups over HTTP: the invite link joins nobody until the click, a visitor joins as a guest,
//! and nobody else sees anything of a group.

use super::*;
use crate::api::{
    CreateGroup, CurrentUser, GroupBoards, GroupInvite, JoinGroup, LeaveGroup, LoadGroup,
    LoadGroupWeek, MyGroups, RemoveMember, RotateInvite,
};

async fn users(conn: &Connection) -> i64 {
    let mut rows = conn.query("SELECT COUNT(*) FROM users", ()).await.unwrap();
    rows.next().await.unwrap().unwrap().get(0).unwrap()
}

#[tokio::test]
async fn friends_join_with_the_link_and_only_they_see_the_group() {
    let server = Server::start().await;
    let conn = conn().await;
    let owner = Browser::signed_up(&server, "group_owner").await;
    let page = owner
        .call(CreateGroup {
            name: "  Bridge   Club ".into(),
        })
        .await
        .unwrap();
    assert_eq!(page.name, "Bridge Club");
    let code = page.invite.clone();

    // A visitor opens the link: the page and the preview make no user and set no cookie
    let visitor = server.browser();
    let before = users(&conn).await;
    assert_status(&visitor.get(&format!("/g/{code}")).await, StatusCode::OK);
    let called = visitor
        .call_with_headers(GroupInvite { code: code.clone() })
        .await;
    let preview = called.result.unwrap().unwrap();
    assert_eq!(
        (preview.name.as_str(), preview.members.len()),
        ("Bridge Club", 1)
    );
    assert_eq!(preview.joined, None);
    assert!(called.headers.get(reqwest::header::SET_COOKIE).is_none());
    assert_eq!(users(&conn).await, before);
    assert_eq!(visitor.call(MyGroups {}).await.unwrap(), vec![]);
    assert_eq!(visitor.call(LoadGroup { id: page.id }).await.unwrap(), None);

    // The click makes them a guest, in the group
    let id = visitor
        .call(JoinGroup { code: code.clone() })
        .await
        .unwrap();
    assert_eq!(id, page.id);
    assert!(visitor.session().is_some());
    let me = visitor.call(CurrentUser {}).await.unwrap().unwrap();
    assert!(me.guest);
    let seen = visitor.call(LoadGroup { id }).await.unwrap().unwrap();
    assert_eq!(seen.members.len(), 2);
    assert_eq!(seen.me, me.id);
    assert!(!seen.is_owner());
    let mine = visitor.call(MyGroups {}).await.unwrap();
    assert_eq!(mine.iter().map(|g| g.id).collect::<Vec<_>>(), [id]);
    // Nobody finished today's daily here: the board is closed
    let boards = visitor.call(GroupBoards {}).await.unwrap();
    assert_eq!(boards.len(), 1);
    assert!(!boards[0].open);

    // Someone else: nothing to see, and it looks like no group at all
    let stranger = Browser::signed_up(&server, "group_stranger").await;
    assert_eq!(stranger.call(LoadGroup { id }).await.unwrap(), None);
    assert_eq!(
        stranger.call(LoadGroup { id: id + 1000 }).await.unwrap(),
        None
    );
    assert_eq!(
        api_error(stranger.call(LoadGroupWeek { id, monday: None }).await),
        ApiError::GroupNotFound
    );
    assert_eq!(
        api_error(stranger.call(LeaveGroup { id }).await),
        ApiError::GroupNotFound
    );
    // Without a session nothing changes
    assert_eq!(
        api_error(server.browser().call(RotateInvite { id }).await),
        ApiError::SessionExpired
    );

    // Members can't do the owner's part
    assert_eq!(
        api_error(visitor.call(RotateInvite { id }).await),
        ApiError::GroupOwnerOnly
    );
    let week = visitor
        .call(LoadGroupWeek { id, monday: None })
        .await
        .unwrap();
    assert_eq!(week.rows.len(), 2);

    // A new link: the old one leads nowhere
    let new = owner.call(RotateInvite { id }).await.unwrap();
    assert_ne!(new, code);
    assert_eq!(
        stranger
            .call(GroupInvite { code: code.clone() })
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        api_error(stranger.call(JoinGroup { code }).await),
        ApiError::GroupNotFound
    );

    // The owner removes the guest, who's out
    owner
        .call(RemoveMember { id, member: me.id })
        .await
        .unwrap();
    assert_eq!(visitor.call(LoadGroup { id }).await.unwrap(), None);
    // The owner leaves last: the group is gone
    owner.call(LeaveGroup { id }).await.unwrap();
    assert_eq!(owner.call(LoadGroup { id }).await.unwrap(), None);
    assert_eq!(
        stranger.call(GroupInvite { code: new }).await.unwrap(),
        None
    );
}

#[tokio::test]
async fn group_names_are_checked() {
    let server = Server::start().await;
    let browser = server.browser();
    assert_eq!(
        api_error(browser.call(CreateGroup { name: "   ".into() }).await),
        ApiError::GroupNameInvalid
    );
    assert_eq!(
        api_error(
            browser
                .call(CreateGroup {
                    name: "Shit Club".into()
                })
                .await
        ),
        ApiError::GroupNameNotAllowed
    );
}
