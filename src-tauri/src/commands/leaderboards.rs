use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use reqwest::Url;
use crate::server_api;
use serde::{Deserialize, Serialize};

const LOCAL_API_BIND: &str = "127.0.0.1:50053";
const LIST_ROUTE: &str = "/v1/leaderboards";
const SUBMIT_ROUTE: &str = "/v1/leaderboards/{board_id}/scores";
const MAX_BOARD_ID_LENGTH: usize = 32;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RankedEntry
{
	pub rank: usize,
	pub player_name: String,
	pub score: i64,
	pub submitted_at: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Leaderboard
{
	pub id: String,
	pub name: String,
	pub order: String,
	#[serde(default = "default_enabled")]
	pub enabled: bool,
	pub entries: Vec<RankedEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LeaderboardListResponse
{
	game_id: String,
	leaderboards: Vec<Leaderboard>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LocalLeaderboardListResponse
{
	leaderboards: Vec<Leaderboard>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SyncLeaderboardDefinition
{
	name: String,
	order: String,
	enabled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SyncLeaderboardsRequest
{
	leaderboards: Vec<SyncLeaderboardDefinition>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ScoreSubmission
{
	player_name: String,
	score: i64,
}

#[derive(Debug, Deserialize, Serialize)]
struct ScoreSubmissionResponse
{
	ok: bool,
	rank: usize,
}

#[derive(Debug, Serialize)]
struct ProxyError
{
	ok: bool,
	message: String,
}

#[derive(Debug, Deserialize)]
struct ServerError
{
	message: String,
}

fn authorized_game_id(
	headers: &HeaderMap,
	process_state: &crate::commands::launch::GameProcessState,
) -> Result<String, (StatusCode, Json<ProxyError>)>
{
	let token = headers
		.get(axum::http::header::AUTHORIZATION)
		.and_then(|value| value.to_str().ok())
		.and_then(|value| value.strip_prefix("Bearer "))
		.filter(|value| !value.is_empty())
		.ok_or_else(unauthorized)?;
	process_state
		.game_id_for_session(token)
		.map_err(proxy_error)?
		.ok_or_else(unauthorized)
}

fn unauthorized() -> (StatusCode, Json<ProxyError>)
{
	(StatusCode::UNAUTHORIZED, Json(ProxyError {
		ok: false,
		message: "このゲームセッションはランキングAPIを利用できません".to_string(),
	}))
}

fn default_enabled() -> bool { true }

fn server_endpoint(game_id: &str, board_id: Option<&str>) -> Result<Url, String>
{
	match board_id
	{
		Some(board_id) => server_api::game_endpoint(game_id, &["leaderboards", board_id, "scores"]),
		None => server_api::game_endpoint(game_id, &["leaderboards"]),
	}
}

fn validate_board_id(board_id: &str) -> Result<(), String>
{
	let has_valid_characters = board_id.bytes()
		.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-');
	let is_valid = !board_id.is_empty()
		&& board_id.len() <= MAX_BOARD_ID_LENGTH
		&& has_valid_characters;
	if is_valid { Ok(()) } else { Err("ランキングIDが不正です".to_string()) }
}

async fn fetch_from_server(game_id: &str) -> Result<LeaderboardListResponse, String>
{
	let game_id = crate::env::normalize_game_id(game_id)?;
	let response = server_api::client()?
		.get(server_endpoint(&game_id, None)?)
		.send()
		.await
		.map_err(|error| format!("ランキングServerに接続できません: {error}"))?;

	if !response.status().is_success()
	{
		return Err(format!("ランキングを取得できません (HTTP {})", response.status()));
	}

	let mut result: LeaderboardListResponse = response.json().await
		.map_err(|error| format!("ランキングの応答が不正です: {error}"))?;
	result.leaderboards.truncate(2);
	for board in &mut result.leaderboards
	{
		board.entries.truncate(10);
	}
	Ok(result)
}

async fn submit_to_server(
	game_id: &str,
	board_id: &str,
	submission: ScoreSubmission,
) -> Result<ScoreSubmissionResponse, String>
{
	let game_id = crate::env::normalize_game_id(game_id)?;
	validate_board_id(board_id)?;

	let response = server_api::client()?
		.post(server_endpoint(&game_id, Some(board_id))?)
		.json(&submission)
		.send()
		.await
		.map_err(|error| format!("ランキングServerに接続できません: {error}"))?;
	let status = response.status();

	if !status.is_success()
	{
		let message = response.json::<ServerError>().await
			.map(|error| error.message)
			.unwrap_or_else(|_| format!("スコアを登録できません (HTTP {status})"));
		return Err(message);
	}

	response.json().await
		.map_err(|error| format!("ランキングの応答が不正です: {error}"))
}

async fn sync_with_server(
	game_id: &str,
	request: SyncLeaderboardsRequest,
) -> Result<LeaderboardListResponse, String>
{
	let game_id = crate::env::normalize_game_id(game_id)?;
	let response = server_api::client()?
		.put(server_endpoint(&game_id, None)?)
		.json(&request)
		.send()
		.await
		.map_err(|error| format!("ランキングServerに接続できません: {error}"))?;
	let status = response.status();
	if !status.is_success()
	{
		let message = response.json::<ServerError>().await
			.map(|error| error.message)
			.unwrap_or_else(|_| format!("ランキングを同期できません (HTTP {status})"));
		return Err(message);
	}
	response.json().await
		.map_err(|error| format!("ランキングの応答が不正です: {error}"))
}

fn expose_slots(mut result: LeaderboardListResponse) -> LocalLeaderboardListResponse
{
	for (index, board) in result.leaderboards.iter_mut().enumerate()
	{
		board.id = index.to_string();
	}
	LocalLeaderboardListResponse { leaderboards: result.leaderboards }
}

#[tauri::command]
pub async fn get_leaderboards(game_id: String) -> Result<Vec<Leaderboard>, String>
{
	Ok(fetch_from_server(&game_id).await?.leaderboards)
}

type ProxyResult<T> = Result<Json<T>, (StatusCode, Json<ProxyError>)>;

fn proxy_error(message: String) -> (StatusCode, Json<ProxyError>)
{
	(StatusCode::BAD_GATEWAY, Json(ProxyError { ok: false, message }))
}

async fn proxy_list(
	State(process_state): State<crate::commands::launch::GameProcessState>,
	headers: HeaderMap,
) -> ProxyResult<LocalLeaderboardListResponse>
{
	let game_id = authorized_game_id(&headers, &process_state)?;
	let result = fetch_from_server(&game_id).await.map_err(proxy_error)?;
	Ok(Json(expose_slots(result)))
}

async fn proxy_sync(
	State(process_state): State<crate::commands::launch::GameProcessState>,
	headers: HeaderMap,
	Json(request): Json<SyncLeaderboardsRequest>,
) -> ProxyResult<LocalLeaderboardListResponse>
{
	let game_id = authorized_game_id(&headers, &process_state)?;
	let result = sync_with_server(&game_id, request).await.map_err(proxy_error)?;
	Ok(Json(expose_slots(result)))
}

async fn proxy_submit(
	State(process_state): State<crate::commands::launch::GameProcessState>,
	headers: HeaderMap,
	Path(board_slot): Path<usize>,
	Json(submission): Json<ScoreSubmission>,
) -> ProxyResult<ScoreSubmissionResponse>
{
	let game_id = authorized_game_id(&headers, &process_state)?;
	if board_slot > 1
	{
		return Err(proxy_error("ランキング番号は0または1で指定してください".to_string()));
	}
	let boards = fetch_from_server(&game_id).await.map_err(proxy_error)?;
	let board_id = boards.leaderboards.get(board_slot)
		.map(|board| board.id.as_str())
		.ok_or_else(|| proxy_error("指定したランキングはまだ同期されていません".to_string()))?;
	let result = submit_to_server(&game_id, board_id, submission)
		.await
		.map_err(proxy_error)?;
	Ok(Json(result))
}

pub async fn serve_local_api(process_state: crate::commands::launch::GameProcessState)
{
	let app = Router::new()
		.route(LIST_ROUTE, get(proxy_list).put(proxy_sync))
		.route(SUBMIT_ROUTE, post(proxy_submit))
		.with_state(process_state);
	let listener = match tokio::net::TcpListener::bind(LOCAL_API_BIND).await
	{
		Ok(listener) => listener,
		Err(error) => {
			eprintln!("Unity leaderboard APIを起動できません: {error}");
			return;
		}
	};

	println!("Unity leaderboard API listening on http://{LOCAL_API_BIND}");
	if let Err(error) = axum::serve(listener, app).await
	{
		eprintln!("Unity leaderboard API error: {error}");
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn server_board_ids_are_hidden_behind_slots()
	{
		let result = expose_slots(LeaderboardListResponse {
			game_id: "game".into(),
			leaderboards: vec![
				Leaderboard { id: "old_score".into(), name: "Score".into(), order: "high_score".into(), enabled: true, entries: vec![] },
				Leaderboard { id: "old_time".into(), name: "Time".into(), order: "low_score".into(), enabled: true, entries: vec![] },
			],
		});
		assert_eq!(result.leaderboards[0].id, "0");
		assert_eq!(result.leaderboards[1].id, "1");
	}
}
