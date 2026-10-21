//! Seasons of the SAT Dojo: the commitment to a season's hidden set, freezing its results when
//! the next one starts, revealing its seed and publishing its set and per-formula results, like
//! the SAT Competition publishes its instances and `scores.csv` every year.
//!
//! - The start of a season is recorded with the commitment to its seed, `sha256(seed)` (the
//!   seed's text, see `dojo::archive::commitment`), shown on the leaderboard from then on. The
//!   seed itself comes from `DOJO_SECRET` and can't be changed later without breaking the
//!   commitment.
//! - When `DOJO_SEASON` moves on, housekeeping freezes every earlier season before anything is
//!   evaluated again: the results of its finished submissions go to `dojo_archive` with their
//!   final rank, and its seed is revealed. In one transaction with re-queueing its submissions,
//!   so a result that arrives in between can't get lost, and safe to run twice.
//! - A new [`dojo::RULES_VERSION`] re-evaluates the finished submissions of the current season,
//!   without archiving anything or changing the season.
//! - The set: regenerating it from the revealed seed only works while the generator is
//!   unchanged, so the site keeps the set itself. The worker has it at hand when it evaluates
//!   (cached, already generated), so the site asks for it with a job (`export=1`) while it has
//!   none of this season from the deployed generator, and stores the tarball the worker posts
//!   back (`POST /dojo/set`, authenticated with the job's token like its reports). That needs no
//!   extra generation (minutes), no long request to the worker and no key on the worker beyond
//!   the job's, and the set is stored while the worker that evaluates on it is the deployed one:
//!   after a deploy with a new generator nobody could make the old set any more. It stays private
//!   until the season is over, then it is served at `/dojo/seasons/<n>/set.tar.gz`.
//! - Per-formula results of archived seasons: `/dojo/seasons/<n>/results.csv` and `.json`. The
//!   current season's stay per family only, as the guide promises.

use std::fmt::Write as _;

use axum::body::Bytes;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use dojo::{GENERATOR_VERSION, InstanceResult, RULES_VERSION, Score};
use leptos::prelude::ServerFnError;
use libsql::{Connection, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::board::ranked;
use super::metrics::{self, results_of};
use super::{season, test_seed};
use crate::model::{DojoEntry, DojoPortfolio, DojoSeason, DojoSeasons};
use crate::server::db::{self, db_err};
use crate::server::{hex, now_ms};

/// Largest set tarball the worker may post (a set is about 1.5 MB).
pub const MAX_SET_BYTES: usize = 32 << 20;

/// The commitment to a seed: the SHA-256 of its text, in hex. The same as
/// `dojo::archive::commitment`, which the site doesn't build.
pub fn commitment(seed: &str) -> String {
    hex(&Sha256::digest(seed.as_bytes()))
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// Records the start of the current season with its commitment (once), and the versions of
/// the generator and the rules it runs with now. When those change, the season's failed
/// evaluations get another chance, with their attempts back: whatever made them fail may be
/// gone with the new rules (finished ones are evaluated again by [`requeue_old_rules`]).
pub(super) async fn open(conn: &Connection, current: u32, now: i64) -> Result<(), String> {
    conn.execute(
        "INSERT OR IGNORE INTO dojo_seasons (season, started_at, commitment, generator_version, rules_version)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            current,
            now,
            commitment(&test_seed(current)),
            GENERATOR_VERSION,
            RULES_VERSION
        ],
    )
    .await
    .map_err(err)?;
    // Both or neither: another housekeeping run at the same moment finds the versions changed
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(err)?;
    let changed = tx
        .execute(
            "UPDATE dojo_seasons SET generator_version = ?2, rules_version = ?3
             WHERE season = ?1 AND ended_at IS NULL AND (generator_version != ?2 OR rules_version != ?3)",
            params![current, GENERATOR_VERSION, RULES_VERSION],
        )
        .await
        .map_err(err)?;
    if changed > 0 {
        tx.execute(
            "UPDATE dojo_submissions SET state = 'queued', progress = 0, updated_at = ?2, attempts = 0,
                fingerprint = NULL, similar_to = NULL, results = NULL, error = NULL, job_token = NULL
             WHERE season = ?1 AND state = 'failed'",
            params![current, now],
        )
        .await
        .map_err(err)?;
    }
    tx.commit().await.map_err(err)?;
    // Its draw from the community pool, fixed from now on
    super::benchmarks::draw(conn, current, now).await
}

/// Freezes every season before `current` that still has submissions in it, or isn't marked
/// as over yet.
pub(super) async fn freeze_past(conn: &Connection, current: u32, now: i64) -> Result<(), String> {
    let mut rows = conn
        .query(
            "SELECT season FROM dojo_submissions WHERE season < ?1 AND state IN ('done', 'queued', 'running')
             UNION SELECT season FROM dojo_seasons WHERE season < ?1 AND ended_at IS NULL",
            params![current],
        )
        .await
        .map_err(err)?;
    let mut seasons = Vec::new();
    while let Some(row) = rows.next().await.map_err(err)? {
        seasons.push(row.get::<u32>(0).map_err(err)?);
    }
    drop(rows);
    for season in seasons {
        freeze(conn, season, current, now).await?;
    }
    Ok(())
}

/// Archives the finished submissions of `season` with their final rank, reveals its seed and
/// moves its submissions to `current` to be evaluated again. All or nothing: another
/// housekeeping run at the same moment waits, then finds nothing left to do.
async fn freeze(conn: &Connection, season: u32, current: u32, now: i64) -> Result<(), String> {
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .await
        .map_err(err)?;
    let ranks = ranked(&tx, season).await.map_err(err)?;
    // Seasons from before the archive have no row yet
    tx.execute(
        "INSERT OR IGNORE INTO dojo_seasons (season, started_at, commitment, generator_version, rules_version)
         VALUES (?1, COALESCE((SELECT MIN(created_at) FROM dojo_submissions WHERE season = ?1), ?2), ?3, ?4, ?5)",
        params![
            season,
            now,
            commitment(&test_seed(season)),
            GENERATOR_VERSION,
            RULES_VERSION
        ],
    )
    .await
    .map_err(err)?;
    tx.execute(
        "INSERT OR IGNORE INTO dojo_archive (season, submission_id, user_id, house, name, source_url, created_at,
            par2, solved, wrong, score, fingerprint, results, rules_version)
         SELECT season, id, user_id, house, name, source_url, created_at, par2, solved, wrong, score,
            fingerprint, results, rules_version
         FROM dojo_submissions WHERE season = ?1 AND state = 'done'",
        params![season],
    )
    .await
    .map_err(err)?;
    for r in ranks {
        tx.execute(
            "UPDATE dojo_archive SET rank = ?3 WHERE season = ?1 AND submission_id = ?2",
            params![season, r.entry.id, r.entry.rank],
        )
        .await
        .map_err(err)?;
    }
    tx.execute(
        "UPDATE dojo_seasons SET ended_at = COALESCE(ended_at, ?2), seed = COALESCE(seed, ?3) WHERE season = ?1",
        params![season, now, test_seed(season)],
    )
    .await
    .map_err(err)?;
    tx.execute(
        "UPDATE dojo_submissions SET state = 'queued', season = ?2, progress = 0, updated_at = ?3, attempts = 0,
            fingerprint = NULL, similar_to = NULL, results = NULL, job_token = NULL
         WHERE season = ?1 AND state IN ('done', 'queued', 'running')",
        params![season, current, now],
    )
    .await
    .map_err(err)?;
    tx.commit().await.map_err(err)
}

/// Evaluates the finished submissions of the season again whose results are from older rules
/// than the site's (see [`RULES_VERSION`]).
pub(super) async fn requeue_old_rules(
    conn: &Connection,
    current: u32,
    now: i64,
) -> Result<u64, String> {
    conn.execute(
        "UPDATE dojo_submissions SET state = 'queued', progress = 0, updated_at = ?3, attempts = 0,
            fingerprint = NULL, similar_to = NULL, results = NULL, job_token = NULL
         WHERE season = ?1 AND state = 'done' AND COALESCE(rules_version, 0) < ?2",
        params![current, RULES_VERSION, now],
    )
    .await
    .map_err(err)
}

/// Whether the worker should send the set with the next job: the site has none of this season,
/// or one from another generator.
pub(super) async fn wants_set(conn: &Connection, current: u32) -> Result<bool, String> {
    let mut rows = conn
        .query(
            "SELECT generator_version FROM dojo_season_sets WHERE season = ?1",
            params![current],
        )
        .await
        .map_err(err)?;
    Ok(match rows.next().await.map_err(err)? {
        Some(row) => row.get::<u32>(0).map_err(err)? != GENERATOR_VERSION,
        None => true,
    })
}

#[derive(Debug, Deserialize)]
pub struct SetQuery {
    pub(super) id: i64,
    pub(super) season: u32,
    pub(super) generator: u32,
}

/// `POST /dojo/set?id=&season=&generator=` from the worker: the set of a job of the current
/// season, as a tarball, with the job's token. A newer generator's replaces an older one's.
pub async fn set_handler(headers: HeaderMap, Query(q): Query<SetQuery>, body: Bytes) -> StatusCode {
    let Ok(conn) = db::conn() else {
        return StatusCode::SERVICE_UNAVAILABLE;
    };
    store_set(&conn, &headers, &q, &body, season()).await
}

pub(super) async fn store_set(
    conn: &Connection,
    headers: &HeaderMap,
    q: &SetQuery,
    body: &[u8],
    current: u32,
) -> StatusCode {
    match super::worker::job_authorized(conn, q.id, headers).await {
        Ok(Some(season)) if season == q.season && season == current => {}
        // Another season by now: that set is either stored already or never was the current one
        Ok(Some(_)) => return StatusCode::CONFLICT,
        Ok(None) => return StatusCode::UNAUTHORIZED,
        Err(status) => return status,
    }
    // gzip, nothing else
    if !body.starts_with(&[0x1f, 0x8b]) {
        return StatusCode::UNPROCESSABLE_ENTITY;
    }
    let stored = conn
        .execute(
            "INSERT INTO dojo_season_sets (season, generator_version, archive, sha256, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (season) DO UPDATE SET generator_version = excluded.generator_version,
                archive = excluded.archive, sha256 = excluded.sha256, created_at = excluded.created_at
             WHERE excluded.generator_version >= dojo_season_sets.generator_version",
            params![
                q.season,
                q.generator,
                body.to_vec(),
                hex(&Sha256::digest(body)),
                now_ms()
            ],
        )
        .await;
    match stored {
        Ok(_) => StatusCode::NO_CONTENT,
        Err(e) => {
            eprintln!("dojo set: {e}");
            StatusCode::INTERNAL_SERVER_ERROR
        }
    }
}

fn score_of(json: Option<String>) -> Option<Score> {
    json.and_then(|s| serde_json::from_str(&s).ok())
}

/// The commitment of a season as recorded at its start, or worked out if it wasn't yet.
pub(super) async fn commitment_of(conn: &Connection, season: u32) -> Result<String, ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT commitment FROM dojo_seasons WHERE season = ?1",
            params![season],
        )
        .await
        .map_err(db_err)?;
    Ok(match rows.next().await.map_err(db_err)? {
        Some(row) => row.get(0).map_err(db_err)?,
        None => commitment(&test_seed(season)),
    })
}

/// A season's row as [`DojoSeason`], without its board. The seed only for seasons before
/// `current`: the current one's is never revealed, whatever the table says.
async fn season_rows(
    conn: &Connection,
    current: u32,
    filter: &str,
    season: u32,
) -> Result<Vec<DojoSeason>, ServerFnError> {
    let mut rows = conn
        .query(
            &format!(
                "SELECT s.season, s.started_at, s.ended_at, s.commitment, s.seed, s.generator_version,
                    s.rules_version, LENGTH(t.archive), t.sha256,
                    (SELECT COUNT(*) FROM dojo_archive a WHERE a.season = s.season)
                 FROM dojo_seasons s LEFT JOIN dojo_season_sets t ON t.season = s.season
                 WHERE {filter} ORDER BY s.season DESC"
            ),
            params![current, season],
        )
        .await
        .map_err(db_err)?;
    let mut seasons = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let number: u32 = row.get(0).map_err(db_err)?;
        let over = number < current;
        let seed = row
            .get::<Option<String>>(4)
            .map_err(db_err)?
            .filter(|_| over);
        let generator_version: u32 = row.get(5).map_err(db_err)?;
        // Hidden sets have had a held-out family since generator 2
        let mystery = seed
            .as_deref()
            .filter(|_| generator_version >= 2)
            .map(dojo::mystery_family);
        seasons.push(DojoSeason {
            season: number,
            started_at: Some(row.get(1).map_err(db_err)?),
            ended_at: row.get::<Option<i64>>(2).map_err(db_err)?.filter(|_| over),
            commitment: row.get(3).map_err(db_err)?,
            seed,
            generator_version,
            rules_version: row.get(6).map_err(db_err)?,
            set_size: row.get::<Option<i64>>(7).map_err(db_err)?.filter(|_| over),
            set_sha256: row
                .get::<Option<String>>(8)
                .map_err(db_err)?
                .filter(|_| over),
            entries: row.get(9).map_err(db_err)?,
            mystery,
            community: Vec::new(),
            board: Vec::new(),
            portfolio: None,
        });
    }
    Ok(seasons)
}

/// The final board of an archived season: its ranked entries, best first, with their metrics
/// (every family under its own name now), and its portfolio. `rules`: the rules version it
/// was scored with, which says the budget and how far the runs went.
async fn frozen_board(
    conn: &Connection,
    season: u32,
    rules: u32,
) -> Result<(Vec<DojoEntry>, Option<DojoPortfolio>), ServerFnError> {
    let mut rows = conn
        .query(
            "SELECT a.rank, a.submission_id, a.name,
                CASE WHEN a.house != 0 THEN 'Hashi' ELSE COALESCE(u.name, '') END,
                a.house, a.score, a.created_at, a.results
             FROM dojo_archive a LEFT JOIN users u ON u.id = a.user_id
             WHERE a.season = ?1 AND a.rank IS NOT NULL ORDER BY a.rank",
            params![season],
        )
        .await
        .map_err(db_err)?;
    let mut board = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let Some(score) = score_of(row.get(5).map_err(db_err)?) else {
            continue;
        };
        let entry = DojoEntry {
            rank: row.get(0).map_err(db_err)?,
            id: row.get(1).map_err(db_err)?,
            name: row.get(2).map_err(db_err)?,
            author: row.get(3).map_err(db_err)?,
            house: row.get::<i64>(4).map_err(db_err)? != 0,
            score,
            created_at: row.get(6).map_err(db_err)?,
            breakdown: None,
            contribution: None,
        };
        board.push((entry, results_of(row.get(7).map_err(db_err)?)));
    }
    let portfolio = metrics::apply(
        board.iter_mut().map(|(e, rs)| (e, rs.as_slice())),
        dojo::budget_of(rules),
        dojo::run_budget_of(rules),
        true,
    );
    Ok((board.into_iter().map(|(e, _)| e).collect(), portfolio))
}

/// The current season and every finished one, with their final boards.
pub async fn seasons(conn: &Connection, current: u32) -> Result<DojoSeasons, ServerFnError> {
    let current_season = season_rows(conn, current, "s.season = ?1 AND ?2 = ?2", 0)
        .await?
        .pop()
        .unwrap_or_else(|| DojoSeason {
            season: current,
            started_at: None,
            ended_at: None,
            commitment: commitment(&test_seed(current)),
            seed: None,
            generator_version: GENERATOR_VERSION,
            rules_version: RULES_VERSION,
            set_size: None,
            set_sha256: None,
            entries: 0,
            mystery: None,
            community: Vec::new(),
            board: Vec::new(),
            portfolio: None,
        });
    let mut past = season_rows(
        conn,
        current,
        "s.season < ?1 AND s.ended_at IS NOT NULL AND ?2 = ?2",
        0,
    )
    .await?;
    for season in &mut past {
        (season.board, season.portfolio) =
            frozen_board(conn, season.season, season.rules_version).await?;
        season.community = super::benchmarks::drawn(conn, season.season).await?;
    }
    Ok(DojoSeasons {
        current: current_season,
        past,
    })
}

/// An archived season with every result, as `results.json` has it.
#[derive(Debug, Serialize)]
pub struct ArchivedSeason {
    pub season: u32,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
    pub commitment: String,
    pub seed: Option<String>,
    pub generator_version: u32,
    pub rules_version: u32,
    /// Fuel per formula: the official scores count what happened within it.
    pub budget: u64,
    /// How far the runs went, the same as `budget` up to rules version 4. The fuel of a result
    /// is up to this, its outcome how the run ended (see `dojo::Outcome::at` for what it
    /// counts as with less).
    pub run_budget: u64,
    pub set_sha256: Option<String>,
    /// The held-out family of the hidden set (see `dojo::mystery_family`).
    pub mystery: Option<dojo::Family>,
    /// The community benchmarks of the hidden set and their authors (licensed CC BY 4.0).
    pub community: Vec<crate::model::CommunityCredit>,
    /// The virtual best solver of the ranked entries: the best result of any of them on every
    /// formula (see `dojo::metrics::Portfolio`).
    pub vbs: Option<dojo::metrics::Split>,
    pub entries: Vec<ArchivedEntry>,
}

#[derive(Debug, Serialize)]
pub struct ArchivedEntry {
    pub id: i64,
    /// Empty when the author deleted their account.
    pub name: String,
    pub author: String,
    pub house: bool,
    /// On the final board, `None`: not ranked (a wrong answer, nothing solved, a copy, or not
    /// the author's best).
    pub rank: Option<u32>,
    pub source_url: Option<String>,
    pub created_at: i64,
    pub rules_version: Option<u32>,
    pub score: Option<Score>,
    /// Solved and PAR-2 by answer, overall and per family.
    pub breakdown: Option<dojo::metrics::EntryBreakdown>,
    /// What it added to the other ranked entries, `None` if it wasn't ranked.
    pub contribution: Option<dojo::metrics::Contribution>,
    /// Every formula in set order, empty for results from before they were kept.
    pub results: Vec<InstanceResult>,
}

/// A finished season with every result; `None` for the current one, later ones and seasons
/// that aren't archived.
pub async fn archived(
    conn: &Connection,
    season: u32,
    current: u32,
) -> Result<Option<ArchivedSeason>, ServerFnError> {
    let Some(info) = season_rows(
        conn,
        current,
        "s.season = ?2 AND s.season < ?1 AND s.ended_at IS NOT NULL",
        season,
    )
    .await?
    .pop() else {
        return Ok(None);
    };
    let mut rows = conn
        .query(
            "SELECT a.submission_id, a.name,
                CASE WHEN a.house != 0 THEN 'Hashi' ELSE COALESCE(u.name, '') END,
                a.house, a.rank, a.source_url, a.created_at, a.rules_version, a.score, a.results
             FROM dojo_archive a LEFT JOIN users u ON u.id = a.user_id
             WHERE a.season = ?1
             ORDER BY a.rank IS NULL, a.rank, a.par2 IS NULL, a.par2, a.submission_id",
            params![season],
        )
        .await
        .map_err(db_err)?;
    let mut entries = Vec::new();
    while let Some(row) = rows.next().await.map_err(db_err)? {
        let results: Option<String> = row.get(9).map_err(db_err)?;
        entries.push(ArchivedEntry {
            id: row.get(0).map_err(db_err)?,
            name: row.get(1).map_err(db_err)?,
            author: row.get(2).map_err(db_err)?,
            house: row.get::<i64>(3).map_err(db_err)? != 0,
            rank: row.get(4).map_err(db_err)?,
            source_url: row.get(5).map_err(db_err)?,
            created_at: row.get(6).map_err(db_err)?,
            rules_version: row.get(7).map_err(db_err)?,
            score: score_of(row.get(8).map_err(db_err)?),
            breakdown: None,
            contribution: None,
            results: results_of(results),
        });
    }
    // The metrics as the archive page shows them: contributions among the final board
    let budget = dojo::budget_of(info.rules_version);
    let run_budget = dojo::run_budget_of(info.rules_version);
    let mut ranked: Vec<(DojoEntry, Vec<InstanceResult>)> = entries
        .iter()
        .filter(|e| e.rank.is_some())
        .filter_map(|e| {
            let score = e.score.clone()?;
            Some((
                DojoEntry {
                    rank: e.rank.unwrap_or_default(),
                    id: e.id,
                    name: e.name.clone(),
                    author: e.author.clone(),
                    house: e.house,
                    score,
                    created_at: e.created_at,
                    breakdown: None,
                    contribution: None,
                },
                e.results.clone(),
            ))
        })
        .collect();
    let portfolio = metrics::apply(
        ranked.iter_mut().map(|(e, rs)| (e, rs.as_slice())),
        budget,
        run_budget,
        true,
    );
    for entry in &mut entries {
        entry.breakdown = (!entry.results.is_empty())
            .then(|| dojo::metrics::breakdown(&entry.results, budget, true));
        entry.contribution = ranked
            .iter()
            .find(|(e, _)| e.id == entry.id)
            .and_then(|(e, _)| e.contribution);
    }
    Ok(Some(ArchivedSeason {
        season: info.season,
        started_at: info.started_at,
        ended_at: info.ended_at,
        commitment: info.commitment,
        seed: info.seed,
        generator_version: info.generator_version,
        rules_version: info.rules_version,
        budget,
        run_budget,
        set_sha256: info.set_sha256,
        mystery: info.mystery,
        community: super::benchmarks::drawn(conn, season).await?,
        vbs: portfolio.map(|p| p.vbs),
        entries,
    }))
}

/// A CSV field: quoted when needed, and text that a spreadsheet would run as a formula
/// starts with an apostrophe.
fn csv_field(text: &str) -> String {
    let text = if text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{text}")
    } else {
        text.to_string()
    };
    if text.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

/// One line per entry and formula, like the SAT Competition's `scores.csv`.
///
/// `outcome` and `fuel` are how the run ended and what it used, up to `run_budget` (since
/// rules version 5 ten times `budget`); with `budget` a result with more fuel counts as a
/// timeout, see `dojo::Outcome::at`.
pub fn csv(season: &ArchivedSeason) -> String {
    let mut out = String::from(
        "season,entry,solver,author,house,rank,rules_version,instance,family,expected,outcome,fuel,budget,run_budget\n",
    );
    for entry in &season.entries {
        // Per entry: within a season results of older rules may be archived
        let rules = entry.rules_version.unwrap_or(season.rules_version);
        for r in &entry.results {
            let _ = writeln!(
                out,
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                season.season,
                entry.id,
                csv_field(&entry.name),
                csv_field(&entry.author),
                u8::from(entry.house),
                entry.rank.map(|r| r.to_string()).unwrap_or_default(),
                entry
                    .rules_version
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                csv_field(&r.name),
                r.family.id(),
                r.expected.id(),
                r.outcome.id(),
                r.fuel,
                dojo::budget_of(rules),
                dojo::run_budget_of(rules)
            );
        }
    }
    out
}

fn not_found() -> Response {
    (StatusCode::NOT_FOUND, "no such archived season").into_response()
}

fn download(kind: &'static str, name: &str, body: impl IntoResponse) -> Response {
    (
        [
            (header::CONTENT_TYPE, kind.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".to_string()),
        ],
        body,
    )
        .into_response()
}

async fn archived_now(season: u32) -> Result<Option<ArchivedSeason>, StatusCode> {
    let conn = db::conn().map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    archived(&conn, season, super::season())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

/// `GET /dojo/seasons/<n>/results.json`
pub async fn results_json(Path(season): Path<u32>) -> Response {
    match archived_now(season).await {
        Ok(Some(archived)) => download(
            "application/json",
            &format!("dojo-season-{season}-results.json"),
            serde_json::to_string_pretty(&archived).unwrap_or_default(),
        ),
        Ok(None) => not_found(),
        Err(status) => status.into_response(),
    }
}

/// `GET /dojo/seasons/<n>/results.csv`
pub async fn results_csv(Path(season): Path<u32>) -> Response {
    match archived_now(season).await {
        Ok(Some(archived)) => download(
            "text/csv; charset=utf-8",
            &format!("dojo-season-{season}-results.csv"),
            csv(&archived),
        ),
        Ok(None) => not_found(),
        Err(status) => status.into_response(),
    }
}

/// `GET /dojo/seasons/<n>/set.tar.gz`: the archived set of a finished season.
pub async fn set_download(Path(season): Path<u32>) -> Response {
    let Ok(conn) = db::conn() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let set = async {
        let mut rows = conn
            .query(
                "SELECT t.archive FROM dojo_season_sets t JOIN dojo_seasons s ON s.season = t.season
                 WHERE t.season = ?1 AND t.season < ?2 AND s.ended_at IS NOT NULL",
                params![season, super::season()],
            )
            .await?;
        match rows.next().await? {
            Some(row) => row.get::<Vec<u8>>(0).map(Some),
            None => Ok(None),
        }
    };
    match set.await {
        Ok(Some(archive)) => download(
            "application/gzip",
            &format!("dojo-season-{season}.tar.gz"),
            archive,
        ),
        Ok(None) => not_found(),
        Err(e) => {
            eprintln!("dojo set download: {e}");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// `GET /dojo/seasons.json`: what the archive page shows, for scripts.
pub async fn seasons_json() -> Response {
    let Ok(conn) = db::conn() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    match seasons(&conn, season()).await {
        Ok(seasons) => (
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            ],
            serde_json::to_string(&seasons).unwrap_or_default(),
        )
            .into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
