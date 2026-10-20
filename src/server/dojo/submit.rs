//! Submitting to the SAT Dojo: checking the form, storing the module and the HTTP handlers.

use axum::body::Bytes;
use axum::extract::{Multipart, Query};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use dojo::MAX_WASM_BYTES;
use libsql::{Connection, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::board::{board, submitted_today};
use super::{
    DESCRIPTION_MAX, MAX_PENDING, MIN_ACCOUNT_AGE_MS, NAME_MAX, PER_DAY, URL_MAX, dispatch, season,
};
use crate::server::auth::{self, User};
use crate::server::db;
use crate::server::{count, hex, now_ms};

/// Why a submission was refused, as a code for the page (see `dojo.submit.errors` in locales).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitError {
    SignIn,
    Name,
    /// A word of the list of names that aren't welcome (`server/blocked_names.txt`).
    NameNotAllowed,
    Description,
    Url,
    Size,
    NotWasm,
    Duplicate,
    Limit,
    /// The account was created less than a day ago.
    AccountAge,
    /// Enough submissions from this network today, whatever the account.
    NetworkLimit,
    /// The queue is full, try again later.
    Busy,
    Server,
}

impl SubmitError {
    pub fn code(self) -> &'static str {
        match self {
            Self::SignIn => "sign_in",
            Self::Name => "name",
            Self::NameNotAllowed => "name_not_allowed",
            Self::Description => "description",
            Self::Url => "url",
            Self::Size => "size",
            Self::NotWasm => "not_wasm",
            Self::Duplicate => "duplicate",
            Self::Limit => "limit",
            Self::AccountAge => "account_age",
            Self::NetworkLimit => "network_limit",
            Self::Busy => "busy",
            Self::Server => "server",
        }
    }
}

#[derive(Debug, Default)]
pub struct Form {
    pub name: String,
    pub description: String,
    pub source_url: String,
    pub wasm: Vec<u8>,
    /// The hashed network the upload came from (`None` for house entries and local runs).
    pub network: Option<String>,
}

impl Form {
    fn check(&self) -> Result<(), SubmitError> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > NAME_MAX || name.chars().any(char::is_control)
        {
            return Err(SubmitError::Name);
        }
        if crate::server::names::blocked(name).is_some() {
            return Err(SubmitError::NameNotAllowed);
        }
        if self.description.chars().count() > DESCRIPTION_MAX {
            return Err(SubmitError::Description);
        }
        let url = self.source_url.trim();
        if !url.is_empty()
            && (!url.starts_with("https://")
                || url.len() > URL_MAX
                || url.contains(char::is_whitespace))
        {
            return Err(SubmitError::Url);
        }
        if self.wasm.len() > MAX_WASM_BYTES {
            return Err(SubmitError::Size);
        }
        // The worker checks the rest
        if !self.wasm.starts_with(b"\0asm\x01\0\0\0") {
            return Err(SubmitError::NotWasm);
        }
        Ok(())
    }
}

/// SHA-256 of the module without its custom sections (names, debug info, anything appended
/// there): the same code renamed or padded hashes the same. `None` if it isn't WebAssembly.
pub fn code_hash(wasm: &[u8]) -> Option<String> {
    let mut hasher = Sha256::new();
    for payload in wasmparser::Parser::new(0).parse_all(wasm) {
        let payload = payload.ok()?;
        if let Some((id, range)) = payload.as_section()
            && id != 0
        {
            let range = usize::try_from(range.start).ok()?..usize::try_from(range.end).ok()?;
            hasher.update([id]);
            hasher.update(wasm.get(range)?);
        }
    }
    Some(hex(&hasher.finalize()))
}

/// Stores a submission and queues it. `user` is `None` for house entries.
pub async fn submit(
    conn: &Connection,
    user: Option<&User>,
    form: Form,
) -> Result<i64, SubmitError> {
    form.check()?;
    let server = |e: libsql::Error| {
        eprintln!("dojo submit: {e}");
        SubmitError::Server
    };
    if let Some(user) = user {
        if user.username.is_none() {
            return Err(SubmitError::SignIn);
        }
        // Throwaway accounts made to get around the limits have to wait a day
        if user.created_at > now_ms() - MIN_ACCOUNT_AGE_MS {
            return Err(SubmitError::AccountAge);
        }
        let today = submitted_today(conn, user.id)
            .await
            .map_err(|_| SubmitError::Server)?;
        if today >= PER_DAY {
            return Err(SubmitError::Limit);
        }
        if let Some(network) = &form.network {
            let mut rows = conn
                .query(
                    "SELECT COUNT(*) FROM dojo_submissions WHERE ip_hash = ?1",
                    params![network.clone()],
                )
                .await
                .map_err(server)?;
            let from_network: i64 = match rows.next().await.map_err(server)? {
                Some(row) => row.get(0).map_err(server)?,
                None => 0,
            };
            if from_network >= i64::from(PER_DAY) {
                return Err(SubmitError::NetworkLimit);
            }
        }
        let mut rows = conn
            .query(
                "SELECT COUNT(*) FROM dojo_submissions WHERE state IN ('queued', 'running')",
                (),
            )
            .await
            .map_err(server)?;
        let pending: i64 = match rows.next().await.map_err(server)? {
            Some(row) => row.get(0).map_err(server)?,
            None => 0,
        };
        if pending >= MAX_PENDING {
            return Err(SubmitError::Busy);
        }
    }
    let sha256 = hex(&Sha256::digest(&form.wasm));
    // Renaming, padding or adding custom sections doesn't make a different solver
    let code = code_hash(&form.wasm).ok_or(SubmitError::NotWasm)?;
    let mut rows = conn
        .query(
            "SELECT 1 FROM dojo_submissions WHERE sha256 = ?1 OR code_hash = ?2",
            params![sha256.clone(), code.clone()],
        )
        .await
        .map_err(server)?;
    if rows.next().await.map_err(server)?.is_some() {
        return Err(SubmitError::Duplicate);
    }
    let now = now_ms();
    let url = form.source_url.trim();
    conn.execute(
        "INSERT INTO dojo_submissions
            (user_id, house, name, description, source_url, wasm, sha256, size, created_at, updated_at, state, season,
             code_hash, ip_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9, 'queued', ?10, ?11, ?12)",
        params![
            user.map(|u| u.id),
            i64::from(user.is_none()),
            form.name.trim().to_string(),
            form.description.trim().to_string(),
            (!url.is_empty()).then(|| url.to_string()),
            form.wasm.clone(),
            sha256,
            count(form.wasm.len()),
            now,
            season(),
            code,
            form.network.clone()
        ],
    )
    .await
    .map_err(server)?;
    let id = conn.last_insert_rowid();
    // Don't wait for the next round of the dispatcher
    tokio::spawn(async {
        let _ = dispatch().await;
    });
    Ok(id)
}

/// `POST /dojo/submit`, a multipart form (works without JavaScript). Redirects to the
/// submission, or back to the form with an error code.
pub async fn submit_handler(headers: HeaderMap, mut multipart: Multipart) -> Response {
    let mut form = Form::default();
    let mut prefix = String::new();
    let mut too_large = false;
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => {
                too_large = true;
                break;
            }
        };
        let name = field.name().unwrap_or_default().to_string();
        if name.as_str() == "wasm" {
            match field.bytes().await {
                Ok(bytes) => form.wasm = bytes.to_vec(),
                Err(_) => too_large = true,
            }
        } else {
            let text = field.text().await.unwrap_or_default();
            match name.as_str() {
                "name" => form.name = text,
                "description" => form.description = text,
                "source_url" => form.source_url = text,
                // The language prefix to redirect to, e.g. "/de"
                "prefix"
                    if text.len() <= 8
                        && text
                            .chars()
                            .all(|c| c == '/' || c == '-' || c.is_ascii_alphabetic()) =>
                {
                    prefix = text;
                }
                _ => {}
            }
        }
    }
    let result = async {
        if too_large {
            return Err(SubmitError::Size);
        }
        let conn = db::conn().map_err(|_| SubmitError::Server)?;
        let user = auth::user_from_headers(&conn, &headers)
            .await
            .map_err(|_| SubmitError::Server)?
            .ok_or(SubmitError::SignIn)?;
        form.network = crate::server::client_ip(&headers)
            .map(|ip| crate::server::network_hash(&ip, crate::model::day_of(now_ms())));
        submit(&conn, Some(&user), form).await
    }
    .await;
    match result {
        Ok(id) => Redirect::to(&format!("{prefix}/dojo/{id}")).into_response(),
        Err(e) => Redirect::to(&format!("{prefix}/dojo?error={}#submit", e.code())).into_response(),
    }
}

/// `GET /dojo/board.json`: the leaderboard for scripts and papers.
pub async fn board_json() -> Response {
    let board = match db::conn() {
        Ok(conn) => board(&conn).await,
        Err(e) => Err(e),
    };
    match board {
        Ok(board) => (
            [(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")],
            axum::Json(board),
        )
            .into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

#[derive(Debug, Deserialize)]
pub struct HouseQuery {
    name: String,
    #[serde(default)]
    description: String,
    /// The solver's directory in the repository, `dojo/house` by default.
    #[serde(default)]
    source: Option<String>,
}

/// Whether `path` can be a directory of the repository in a link.
fn repo_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 100
        && !path.split('/').any(|part| part.is_empty() || part == "..")
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-' | '_' | '.'))
}

/// `POST /dojo/house?name=[&description=][&source=]` with `HASHI_ADMIN_TOKEN`: adds one of our
/// own solvers to the leaderboard.
///
/// `scripts/dojo-house.sh` uploads them all. `source` is the solver's directory in the
/// repository (`dojo/house` by default, `dojo/reference` for `MiniSat`, `CaDiCaL` and Kissat).
///
/// A new version replaces the house entry of the same name. The same code again is a
/// duplicate (422 `duplicate`), so uploading solvers that didn't change changes nothing.
pub async fn house_handler(
    headers: HeaderMap,
    Query(q): Query<HouseQuery>,
    wasm: Bytes,
) -> Response {
    // The admin token, not the worker's: nothing the worker holds can add entries
    if !crate::server::bearer_ok(&headers, "HASHI_ADMIN_TOKEN") {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let source = q.source.unwrap_or_else(|| "dojo/house".into());
    if !repo_path(&source) {
        return (StatusCode::BAD_REQUEST, "source").into_response();
    }
    let name = q.name.trim().to_string();
    let form = Form {
        name: name.clone(),
        description: q.description,
        source_url: format!("{}/tree/main/{source}", crate::seo::REPO_URL),
        wasm: wasm.to_vec(),
        network: None,
    };
    let Ok(conn) = db::conn() else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    match submit(&conn, None, form).await {
        Ok(id) => match replace_house(&conn, &name, id).await {
            Ok(()) => (StatusCode::CREATED, id.to_string()).into_response(),
            Err(e) => {
                eprintln!("dojo house: {e}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        },
        Err(e) => (StatusCode::UNPROCESSABLE_ENTITY, e.code()).into_response(),
    }
}

/// Deletes the house entries named `name` other than `id`, like the admin's delete (their
/// results in archived seasons stay).
async fn replace_house(conn: &Connection, name: &str, id: i64) -> Result<(), String> {
    let mut rows = conn
        .query(
            "SELECT id FROM dojo_submissions WHERE house = 1 AND name = ?1 AND id != ?2",
            params![name.to_string(), id],
        )
        .await
        .map_err(|e| e.to_string())?;
    let mut older = Vec::new();
    while let Some(row) = rows.next().await.map_err(|e| e.to_string())? {
        older.push(row.get::<i64>(0).map_err(|e| e.to_string())?);
    }
    for old in older {
        crate::server::admin::submission_action(conn, old, crate::model::SubmissionAction::Delete)
            .await
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
