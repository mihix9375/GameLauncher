use std::time::Duration;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

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
struct LeaderboardResponse
{
	game_id: String,
	leaderboards: Vec<Leaderboard>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SubmitScore
{
	player_name: String,
	score: i64,
}

#[derive(Debug, Serialize)]
struct ProxyError
{
	ok: bool,
	message: String,
}

fn endpoint(game_id: &str, board_id: Option<&str>) -> Result<reqwest::Url, String>
{
	let config = crate::env::get_config();
	let mut url = reqwest::Url::parse(&crate::env::normalize_leaderboard_url(&config.leaderboard_url))
		.map_err(|error| format!("ランキングAPIのURLが不正です: {error}"))?;
	{
		let mut segments = url.path_segments_mut().map_err(|_| "ランキングAPIのURLが不正です".to_string())?;
		segments.pop_if_empty().extend(["v1", "games", game_id, "leaderboards"]);
		if let Some(board_id) = board_id { segments.extend([board_id, "scores"]); }
	}
	Ok(url)
}

fn client() -> Result<reqwest::Client, String>
{
	reqwest::Client::builder().timeout(Duration::from_secs(5)).build().map_err(|error| error.to_string())
}

async fn fetch(game_id: &str) -> Result<LeaderboardResponse, String>
{
	let game_id = crate::env::normalize_game_id(game_id)?;
	let response = client()?.get(endpoint(&game_id, None)?).send().await
		.map_err(|error| format!("ランキングServerに接続できません: {error}"))?;
	if !response.status().is_success() { return Err(format!("ランキングを取得できません (HTTP {})", response.status())); }
	let mut result: LeaderboardResponse = response.json().await
		.map_err(|error| format!("ランキングの応答が不正です: {error}"))?;
	result.leaderboards.truncate(2);
	for board in &mut result.leaderboards { board.entries.truncate(10); }
	Ok(result)
}

async fn submit(game_id: &str, board_id: &str, score: SubmitScore) -> Result<serde_json::Value, String>
{
	let game_id = crate::env::normalize_game_id(game_id)?;
	if board_id.is_empty() || board_id.len() > 32 || !board_id.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
	{
		return Err("ランキングIDが不正です".to_string());
	}
	let response = client()?.post(endpoint(&game_id, Some(board_id))?).json(&score).send().await
		.map_err(|error| format!("ランキングServerに接続できません: {error}"))?;
	let status = response.status();
	let body: serde_json::Value = response.json().await.map_err(|error| format!("ランキングの応答が不正です: {error}"))?;
	if !status.is_success() { return Err(body.get("message").and_then(|value| value.as_str()).unwrap_or("スコアを登録できません").to_string()); }
	Ok(body)
}

#[tauri::command]
pub async fn get_leaderboards(game_id: String) -> Result<Vec<Leaderboard>, String>
{
	Ok(fetch(&game_id).await?.leaderboards)
}

type ProxyResult<T> = Result<Json<T>, (StatusCode, Json<ProxyError>)>;

fn proxy_error(message: String) -> (StatusCode, Json<ProxyError>)
{
	(StatusCode::BAD_GATEWAY, Json(ProxyError { ok: false, message }))
}

async fn proxy_get(AxumPath(game_id): AxumPath<String>) -> ProxyResult<LeaderboardResponse>
{
	Ok(Json(fetch(&game_id).await.map_err(proxy_error)?))
}

async fn proxy_submit(
	State(()): State<()>,
	AxumPath((game_id, board_id)): AxumPath<(String, String)>,
	Json(score): Json<SubmitScore>,
) -> ProxyResult<serde_json::Value>
{
	Ok(Json(submit(&game_id, &board_id, score).await.map_err(proxy_error)?))
}

pub async fn serve_local_api()
{
	let app = Router::new()
		.route("/v1/games/{game_id}/leaderboards", get(proxy_get))
		.route("/v1/games/{game_id}/leaderboards/{board_id}/scores", post(proxy_submit))
		.with_state(());
	match tokio::net::TcpListener::bind("127.0.0.1:50053").await
	{
		Ok(listener) => {
			println!("Unity leaderboard API listening on http://127.0.0.1:50053");
			if let Err(error) = axum::serve(listener, app).await { eprintln!("Unity leaderboard API error: {error}"); }
		}
		Err(error) => eprintln!("Unity leaderboard APIを起動できません: {error}"),
	}
}
