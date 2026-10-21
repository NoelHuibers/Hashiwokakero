//! Reading the SAT Dojo: the leaderboard, a submission and the player's own submissions.

use dojo::{DEFAULT_BUDGET, GENERATOR_VERSION, InstanceResult, RULES_VERSION, RUN_BUDGET, Score};
use leptos::prelude::ServerFnError;
use libsql::{Connection, params};

use super::metrics::{self, results_of};
use super::{BOARD_SIZE, PER_DAY, season};
use crate::model::{
    DojoBoard, DojoCurve, DojoEntry, DojoMe, DojoPortfolio, DojoRow, DojoState, DojoSubmission,
    MS_PER_DAY,
};
use crate::server::auth::User;
use crate::server::db::db_err;
use crate::server::now_ms;

fn score_of(json: Option<String>) -> Option<Score> {
    json.and_then(|s| serde_json::from_str(&s).ok())
}

/// An entry of a season's board.
pub(super) struct Ranked {
    pub(super) entry: DojoEntry,
    /// `None` for house entries.
    pub(super) author: Option<i64>,
    /// Per formula, in set order (private while the season runs).
    pub(super) results: Vec<InstanceResult>,
}

/// The best submission of every author this season, ranked, with the author's id and the
/// per-formula results. Banned authors aren't on it.
pub(super) async fn ranked(conn: &Connection, season: u32) -> Result<Vec<Ranked>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT s.id, s.name, COALESCE(u.name, 'Hashi'), s.house, s.score, s.created_at, s.user_id, s.results
             FROM dojo_submissions s LEFT JOIN users u ON u.id = s.user_id
             WHERE s.state = 'done' AND s.season = ?1 AND s.wrong = 0 AND s.solved > 0
               AND s.similar_to IS NULL AND u.banned_at IS NULL
             ORDER BY s.par2 ASC, s.solved DESC, s.created_at ASC",
            params![season],
        )
        .await
        .map_err(db_err)?;
    let mut seen = std::collections::HashSet::new();
    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let house = row.get::<i64>(3).map_err(db_err)? != 0;
        let id: i64 = row.get(0).map_err(db_err)?;
        let user: Option<i64> = row.get(6).map_err(db_err)?;
        // House entries all stay, everyone else with their best
        if !house && !seen.insert(user) {
            continue;
        }
        let Some(score) = score_of(row.get(4).map_err(db_err)?) else {
            continue;
        };
        let entry = DojoEntry {
            rank: u32::try_from(entries.len() + 1).unwrap_or(u32::MAX),
            id,
            name: row.get(1).map_err(db_err)?,
            author: row.get(2).map_err(db_err)?,
            house,
            score,
            created_at: row.get(5).map_err(db_err)?,
            breakdown: None,
            contribution: None,
        };
        entries.push(Ranked {
            entry,
            author: user.filter(|_| !house),
            results: results_of(row.get(7).map_err(db_err)?),
        });
        if entries.len() == BOARD_SIZE {
            break;
        }
    }
    Ok(entries)
}

/// The board of the running season with its metrics (see [`super::metrics`]).
async fn current_board(
    conn: &Connection,
    season: u32,
) -> Result<(Vec<Ranked>, Option<DojoPortfolio>), ServerFnError> {
    let mut ranked = ranked(conn, season).await?;
    let portfolio = metrics::apply(
        ranked
            .iter_mut()
            .map(|r| (&mut r.entry, r.results.as_slice())),
        DEFAULT_BUDGET,
        RUN_BUDGET,
        false,
    );
    Ok((ranked, portfolio))
}

/// The place of a player's best solver on this season's board, if it made the board.
pub async fn rank_of(conn: &Connection, user: i64) -> Result<Option<u32>, ServerFnError> {
    Ok(ranked(conn, season())
        .await?
        .into_iter()
        .find(|r| r.author == Some(user))
        .map(|r| r.entry.rank))
}

pub async fn board(conn: &Connection) -> Result<DojoBoard, ServerFnError> {
    let season = season();
    let (ranked, portfolio) = current_board(conn, season).await?;
    let entries = ranked.into_iter().map(|r| r.entry).collect();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM dojo_submissions WHERE state IN ('queued', 'running')",
            (),
        )
        .await
        .map_err(db_err)?;
    let queued: u32 = match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err)?,
        None => 0,
    };
    // The generated formulas and the season's draw from the community pool
    let drawn = super::benchmarks::drawn(conn, season).await?.len();
    Ok(DojoBoard {
        season,
        formulas: dojo::SET_SIZE + u32::try_from(drawn).unwrap_or(0),
        budget: DEFAULT_BUDGET,
        entries,
        queued,
        commitment: super::seasons::commitment_of(conn, season).await?,
        generator_version: GENERATOR_VERSION,
        rules_version: RULES_VERSION,
        community: super::benchmarks::summary(conn).await?,
        portfolio,
    })
}

pub async fn submission(
    conn: &Connection,
    id: i64,
    me: Option<i64>,
) -> Result<Option<DojoSubmission>, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT s.id, s.name, s.description, s.source_url, COALESCE(u.name, 'Hashi'), s.house,
                    s.created_at, s.state, s.season, s.progress, s.total, s.score, s.error, s.size, s.sha256, s.user_id,
                    s.similar_to, s.results
             FROM dojo_submissions s LEFT JOIN users u ON u.id = s.user_id WHERE s.id = ?1",
            params![id],
        )
        .await
        .map_err(db_err)?;
    let Some(row) = rows.next().await.map_err(db_err)? else {
        return Ok(None);
    };
    let season: u32 = row.get(8).map_err(db_err)?;
    let state = DojoState::from_id(&row.get::<String>(7).map_err(db_err)?);
    let results = results_of(row.get(17).map_err(db_err)?);
    let (board, mut portfolio) = current_board(conn, season).await?;
    let ranked = board.iter().find(|r| r.entry.id == id);
    let rank = ranked.map(|r| r.entry.rank);
    let user: Option<i64> = row.get(15).map_err(db_err)?;
    let error: Option<String> = row.get(12).map_err(db_err)?;
    let name: String = row.get(1).map_err(db_err)?;
    let house = row.get::<i64>(5).map_err(db_err)? != 0;
    let score = score_of(row.get(11).map_err(db_err)?);
    // Its own curve in the season's plot, also when it isn't on the board
    if let (Some(portfolio), None, true) = (
        &mut portfolio,
        ranked,
        state == DojoState::Done && !results.is_empty(),
    ) {
        portfolio.cactus.curves.push(DojoCurve {
            id,
            name: name.clone(),
            house,
            fuel: dojo::metrics::curve(&results),
        });
    }
    let done = state == DojoState::Done && score.is_some();
    Ok(Some(DojoSubmission {
        id,
        name,
        description: row.get(2).map_err(db_err)?,
        source_url: row.get(3).map_err(db_err)?,
        author: row.get(4).map_err(db_err)?,
        house,
        created_at: row.get(6).map_err(db_err)?,
        state,
        season,
        progress: row.get(9).map_err(db_err)?,
        total: row.get(10).map_err(db_err)?,
        score,
        rejection: error.and_then(|e| serde_json::from_str(&e).ok()),
        rank,
        size: row.get(13).map_err(db_err)?,
        sha256: row.get(14).map_err(db_err)?,
        similar_to: row.get(16).map_err(db_err)?,
        mine: user.is_some() && user == me,
        breakdown: (done && !results.is_empty())
            .then(|| dojo::metrics::breakdown(&results, DEFAULT_BUDGET, false)),
        contribution: ranked.and_then(|r| r.entry.contribution),
        portfolio: portfolio.filter(|_| done),
    }))
}

pub(super) async fn submitted_today(conn: &Connection, user: i64) -> Result<u32, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM dojo_submissions WHERE user_id = ?1 AND created_at > ?2",
            params![user, now_ms() - MS_PER_DAY],
        )
        .await
        .map_err(db_err)?;
    Ok(match rows.next().await.map_err(db_err)? {
        Some(row) => row.get::<u32>(0).map_err(db_err)?,
        None => 0,
    })
}

pub async fn me(conn: &Connection, user: Option<&User>) -> Result<DojoMe, ServerFnError> {
    let Some(user) = user else {
        return Ok(DojoMe {
            registered: false,
            left_today: 0,
            submissions: Vec::new(),
        });
    };
    let mut rows = conn
        .query(
            "SELECT id, name, state, created_at, score FROM dojo_submissions
             WHERE user_id = ?1 ORDER BY created_at DESC LIMIT 20",
            params![user.id],
        )
        .await
        .map_err(db_err)?;
    let mut submissions = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        submissions.push(DojoRow {
            id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            state: DojoState::from_id(&row.get::<String>(2).map_err(db_err)?),
            created_at: row.get(3).map_err(db_err)?,
            score: score_of(row.get(4).map_err(db_err)?),
        });
    }
    Ok(DojoMe {
        registered: user.username.is_some(),
        left_today: PER_DAY.saturating_sub(submitted_today(conn, user.id).await?),
        submissions,
    })
}
