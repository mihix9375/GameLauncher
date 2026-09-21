use std::time::Duration;

use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};

const LOCAL_API_BIND: &str = "127.0.0.1:50053";
const LIST_ROUTE: &str = "/v1/games/{game_id}/leaderboards";
const SUBMIT_ROUTE: &str = "/v1/games/{game_id}/leaderboards/{board_id}/scores";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
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
	pub entries: Vec<RankedEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct LeaderboardListResponse
{
	game_id: String,
	leaderboards: Vec<Leaderboard>,
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

fn server_client() -> Result<Client, String>
{
	Client::builder()
		.timeout(REQUEST_TIMEOUT)
		.build()
		.map_err(|error| format!("ランキング用HTTPクライアントを作成できません: {error}"))
}

fn server_endpoint(game_id: &str, board_id: Option<&str>) -> Result<Url, String>
{
	let config = crate::env::get_config();
	let base_url = crate::env::normalize_leaderboard_url(&config.leaderboard_url);
	let mut url = Url::parse(&base_url)
		.map_err(|error| format!("ランキングAPIのURLが不正です: {error}"))?;
	let mut path = url.path_segments_mut()
		.map_err(|_| "ランキングAPIのURLが不正です".to_string())?;

	path.pop_if_empty()
		.extend(["v1", "games", game_id, "leaderboards"]);
	if let Some(board_id) = board_id
	{
		path.extend([board_id, "scores"]);
	}
	drop(path);
	Ok(url)
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
	let response = server_client()?
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

	let response = server_client()?
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

async fn proxy_list(Path(game_id): Path<String>) -> ProxyResult<LeaderboardListResponse>
{
	Ok(Json(fetch_from_server(&game_id).await.map_err(proxy_error)?))
}

async fn proxy_submit(
	Path((game_id, board_id)): Path<(String, String)>,
	Json(submission): Json<ScoreSubmission>,
) -> ProxyResult<ScoreSubmissionResponse>
{
	let result = submit_to_server(&game_id, &board_id, submission)
		.await
		.map_err(proxy_error)?;
	Ok(Json(result))
}

pub async fn serve_local_api()
{
	let app = Router::new()
		.route(LIST_ROUTE, get(proxy_list))
		.route(SUBMIT_ROUTE, post(proxy_submit));
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
