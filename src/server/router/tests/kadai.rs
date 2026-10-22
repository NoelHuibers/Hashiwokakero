//! Group puzzles over HTTP: posting one, playing it like a daily, the author's view, and nobody
//! outside the group sees anything of it.

use common::{Puzzle, SolverConfig};

use super::*;
use crate::api::{
    CreateGroup, GroupPuzzleCheck, GroupPuzzleHint, GroupPuzzleReveal, JoinGroup, LoadGroupPuzzle,
    LoadGroupPuzzles, LoadPuzzleReplay, PostGroupPuzzle, SeenGroupPuzzleKudos, SolvePuzzle,
    StartGroupPuzzle, SubmitGroupPuzzle, WithdrawGroupPuzzle,
};
use crate::model::{MoveEvent, MyPuzzle};

/// A 2 and a 3 joined by a double bridge, the 3 by a single one to a 1 below it: `[2, 1]`.
fn small() -> Puzzle {
    Puzzle::from_grid(&[vec![2, 0, 3], vec![0, 0, 0], vec![0, 0, 1]])
}

#[tokio::test]
async fn a_group_puzzle_is_posted_played_and_watched() {
    let server = Server::start().await;
    let owner = Browser::signed_up(&server, "kadai_owner").await;
    let group = owner
        .call(CreateGroup {
            name: "Puzzle Club".into(),
        })
        .await
        .unwrap();
    let friend = server.browser();
    friend
        .call(JoinGroup {
            code: group.invite.clone(),
        })
        .await
        .unwrap();

    // Two solutions aren't taken, one is
    let square = Puzzle::from_grid(&[vec![3, 0, 3], vec![0, 0, 0], vec![3, 0, 3]]);
    assert_eq!(
        api_error(
            owner
                .call(PostGroupPuzzle {
                    group: group.id,
                    puzzle: square,
                    title: String::new(),
                })
                .await
        ),
        ApiError::PuzzleNotUnique
    );
    let id = owner
        .call(PostGroupPuzzle {
            group: group.id,
            puzzle: small(),
            title: " Tiny ".into(),
        })
        .await
        .unwrap();
    let list = friend
        .call(LoadGroupPuzzles { group: group.id })
        .await
        .unwrap();
    assert_eq!(list.open.len(), 1);
    assert_eq!(list.open[0].title.as_deref(), Some("Tiny"));

    // Before the start: no puzzle. The solvers don't help with it either.
    let page = friend.call(LoadGroupPuzzle { id }).await.unwrap().unwrap();
    assert_eq!((page.summary.mine, page.puzzle), (MyPuzzle::Fresh, None));
    assert_eq!(
        api_error(
            friend
                .call(SolvePuzzle {
                    puzzle: small(),
                    config: SolverConfig::default(),
                    unique: false,
                })
                .await
        ),
        ApiError::PuzzleLocked
    );
    let started = friend.call(StartGroupPuzzle { id }).await.unwrap();
    assert_eq!(started.puzzle, Some(small()));
    assert_eq!(
        friend
            .call(GroupPuzzleCheck {
                id,
                bridges: vec![2, 0],
            })
            .await
            .unwrap(),
        Vec::<usize>::new()
    );
    assert_eq!(
        friend
            .call(GroupPuzzleHint {
                id,
                bridges: vec![2, 0],
            })
            .await
            .unwrap(),
        Some((1, 1))
    );
    let log = vec![
        MoveEvent {
            t: 10,
            edge: 0,
            count: 1,
            tap: true,
        },
        MoveEvent {
            t: 20,
            edge: 0,
            count: 2,
            tap: true,
        },
        MoveEvent {
            t: 30,
            edge: 1,
            count: 1,
            tap: false,
        },
    ];
    let done = friend
        .call(SubmitGroupPuzzle {
            id,
            bridges: vec![2, 1],
            moves: 2,
            log,
        })
        .await
        .unwrap();
    assert!(matches!(done.summary.mine, MyPuzzle::Solved { .. }));
    let attempt = done.attempt.unwrap();
    assert_eq!((attempt.hints, attempt.checks), (1, 1));
    assert!(done.board_open);
    let replay = friend.call(LoadPuzzleReplay { id }).await.unwrap().unwrap();
    assert!(!replay.locked);
    assert_eq!(replay.lanes.len(), 1);

    // The author can't play, sees the stats, and only they withdraw
    assert_eq!(
        api_error(owner.call(StartGroupPuzzle { id }).await),
        ApiError::PuzzleOwn
    );
    let mine = owner.call(LoadGroupPuzzle { id }).await.unwrap().unwrap();
    assert_eq!(mine.stats.map(|s| (s.solved, s.hints)), Some((1, 1)));
    owner.call(SeenGroupPuzzleKudos { id }).await.unwrap();
    assert_eq!(
        api_error(friend.call(WithdrawGroupPuzzle { id }).await),
        ApiError::PuzzleAuthorOnly
    );

    // Nobody else sees a thing
    let stranger = Browser::signed_up(&server, "kadai_stranger").await;
    assert_eq!(stranger.call(LoadGroupPuzzle { id }).await.unwrap(), None);
    assert_eq!(stranger.call(LoadPuzzleReplay { id }).await.unwrap(), None);
    assert_eq!(
        api_error(stranger.call(LoadGroupPuzzles { group: group.id }).await),
        ApiError::GroupNotFound
    );
    assert_eq!(
        server.browser().call(LoadGroupPuzzle { id }).await.unwrap(),
        None
    );

    // A second one, given up; then both withdrawn
    let second = owner
        .call(PostGroupPuzzle {
            group: group.id,
            puzzle: Puzzle::from_grid(&[vec![1, 0, 1], vec![0, 0, 0]]),
            title: String::new(),
        })
        .await
        .unwrap();
    friend.call(StartGroupPuzzle { id: second }).await.unwrap();
    let given_up = friend.call(GroupPuzzleReveal { id: second }).await.unwrap();
    assert_eq!(given_up.summary.mine, MyPuzzle::GaveUp);
    for id in [id, second] {
        owner.call(WithdrawGroupPuzzle { id }).await.unwrap();
    }
    assert_eq!(friend.call(LoadGroupPuzzle { id }).await.unwrap(), None);
    assert_eq!(friend.call(LoadPuzzleReplay { id }).await.unwrap(), None);
}
