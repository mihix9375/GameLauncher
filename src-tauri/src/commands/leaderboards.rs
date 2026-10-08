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

// 旧Serverの整数応答も読み取り、JSへは桁落ちしない文字列で渡す。
fn score_text<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
	#[derive(Deserialize)] #[serde(untagged)] enum Input { Text(String), Signed(i64), Unsigned(u64) }
	match Input::deserialize(deserializer)? {
		Input::Text(value) => Ok(value),
		Input::Signed(value) => Ok(value.to_string()),
		Input::Unsigned(value) => Ok(value.to_string()),
	}
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RankedEntry
{
	pub rank: usize,
	pub player_name: String,
	#[serde(deserialize_with = "score_text")]
	pub score: String,
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
	#[serde(deserialize_with = "score_text")]
	score: String,
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

#[derive(Debug)]
struct RankingRequestError { status: StatusCode, message: String }
impl From<String> for RankingRequestError {
	fn from(message: String) -> Self { Self { status: StatusCode::BAD_GATEWAY, message } }
}
fn bad_ranking_request(message: String) -> RankingRequestError {
	RankingRequestError { status: StatusCode::BAD_REQUEST, message }
}
async fn server_error(response: reqwest::Response, operation: &str) -> RankingRequestError {
	let status = StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
	let fallback = format!("{operation} (HTTP {status})");
	let message = match response.text().await {
		Ok(body) => serde_json::from_str::<ServerError>(&body).map(|error| error.message)
			.unwrap_or_else(|_| if body.trim().is_empty() { fallback.clone() } else { body.chars().take(4096).collect() }),
		Err(_) => fallback,
	};
	RankingRequestError { status, message }
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

async fn fetch_from_server(game_id: &str) -> Result<LeaderboardListResponse, RankingRequestError>
{
	let game_id = crate::env::normalize_game_id(game_id)?;
	let response = server_api::client()?
		.get(server_endpoint(&game_id, None)?)
		.send()
		.await
		.map_err(|error| format!("ランキングServerに接続できません: {error}"))?;

	if !response.status().is_success()
	{
		return Err(server_error(response, "ランキングを取得できません").await);
	}

	decode_list_response(response, &game_id).await
}

async fn decode_list_response(response: reqwest::Response, game_id: &str) -> Result<LeaderboardListResponse, RankingRequestError> {
	let result: LeaderboardListResponse = response.json().await
		.map_err(|error| format!("ランキングの応答が不正です: {error}"))?;
	validate_server_list(&result, game_id)?;
	Ok(result)
}

fn validate_server_list(result: &LeaderboardListResponse, game_id: &str) -> Result<(), RankingRequestError> {
	if result.game_id != game_id || result.leaderboards.len() > 2 {
		return Err("ランキング応答のゲームIDまたは件数が不正です".to_string().into());
	}
	let mut ids = std::collections::HashSet::new();
	for board in &result.leaderboards {
		validate_board_id(&board.id)?;
		if !ids.insert(&board.id) || board.name.trim().is_empty() || board.name.chars().count() > 40
			|| board.name.chars().any(char::is_control) || !matches!(board.order.as_str(), "high_score" | "low_score")
			|| board.entries.len() > 10 {
			return Err("ランキング応答の設定が不正です".to_string().into());
		}
		for (index, entry) in board.entries.iter().enumerate() {
			if entry.rank != index + 1 || entry.player_name.trim().is_empty() || entry.player_name.chars().count() > 24
				|| entry.player_name.chars().any(char::is_control) || entry.submitted_at < 0 {
				return Err("ランキング応答の記録が不正です".to_string().into());
			}
		}
	}
	Ok(())
}

async fn submit_to_server(
	game_id: &str,
	board_id: &str,
	submission: ScoreSubmission,
) -> Result<ScoreSubmissionResponse, RankingRequestError>
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
		return Err(server_error(response, "スコアを登録できません").await);
	}

	decode_submission_response(response).await
}

async fn decode_submission_response(response: reqwest::Response) -> Result<ScoreSubmissionResponse, RankingRequestError> {
	let result: ScoreSubmissionResponse = response.json().await
		.map_err(|error| format!("ランキングの応答が不正です: {error}"))?;
	if !result.ok || result.rank == 0 {
		return Err("スコア送信の成功を確認できません。自動再送しないでください".to_string().into());
	}
	Ok(result)
}

async fn sync_with_server(
	game_id: &str,
	request: SyncLeaderboardsRequest,
) -> Result<LeaderboardListResponse, RankingRequestError>
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
		return Err(server_error(response, "ランキングを同期できません").await);
	}
	decode_list_response(response, &game_id).await
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
	Ok(fetch_from_server(&game_id).await.map_err(|error| error.message)?.leaderboards)
}

type ProxyResult<T> = Result<Json<T>, (StatusCode, Json<ProxyError>)>;

fn proxy_error(message: String) -> (StatusCode, Json<ProxyError>)
{
	(StatusCode::BAD_GATEWAY, Json(ProxyError { ok: false, message }))
}

fn proxy_request_error(error: RankingRequestError) -> (StatusCode, Json<ProxyError>) {
	(error.status, Json(ProxyError { ok: false, message: error.message }))
}

async fn proxy_list(
	State(process_state): State<crate::commands::launch::GameProcessState>,
	headers: HeaderMap,
) -> ProxyResult<LocalLeaderboardListResponse>
{
	let game_id = authorized_game_id(&headers, &process_state)?;
	let result = fetch_from_server(&game_id).await.map_err(proxy_request_error)?;
	Ok(Json(expose_slots(result)))
}

async fn proxy_sync(
	State(process_state): State<crate::commands::launch::GameProcessState>,
	headers: HeaderMap,
	Json(request): Json<SyncLeaderboardsRequest>,
) -> ProxyResult<LocalLeaderboardListResponse>
{
	let game_id = authorized_game_id(&headers, &process_state)?;
	let result = sync_with_server(&game_id, request).await.map_err(proxy_request_error)?;
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
		return Err(proxy_request_error(bad_ranking_request("ランキング番号は0または1で指定してください".to_string())));
	}
	let boards = fetch_from_server(&game_id).await.map_err(proxy_request_error)?;
	let board_id = boards.leaderboards.get(board_slot)
		.map(|board| board.id.as_str())
		.ok_or_else(|| proxy_request_error(RankingRequestError { status: StatusCode::CONFLICT, message: "指定したランキングはまだ同期されていません".to_string() }))?;
	let result = submit_to_server(&game_id, board_id, submission)
		.await
		.map_err(proxy_request_error)?;
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
			tracing::error!(target: "gamelauncher::ranking", event = "local_api_bind_failed", message = %error);
			return;
		}
	};

	tracing::info!(target: "gamelauncher::ranking", event = "local_api_started", address = LOCAL_API_BIND);
	if let Err(error) = axum::serve(listener, app).await
	{
		tracing::error!(target: "gamelauncher::ranking", event = "local_api_failed", message = %error);
	}
}

#[cfg(test)]
mod tests
{
	use super::*;

	async fn response(status: u16, body: &str) -> reqwest::Response {
		use tokio::io::{AsyncReadExt, AsyncWriteExt};
		let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
		let address = listener.local_addr().unwrap();
		let body = body.to_string();
		tokio::spawn(async move {
			let (mut stream, _) = listener.accept().await.unwrap();
			let mut input = [0; 4096];
			stream.read(&mut input).await.unwrap();
			stream.write_all(format!("HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
		});
		reqwest::Client::new().get(format!("http://{address}")).send().await.unwrap()
	}

	#[tokio::test]
	async fn malformed_lists_are_rejected_not_silently_truncated_or_remapped() {
		let board = r#"{"id":"0","name":"Score","order":"high_score","entries":[]}"#;
		for body in [
			format!(r#"{{"game_id":"other","leaderboards":[{board}]}}"#),
			format!(r#"{{"game_id":"game","leaderboards":[{board},{board}]}}"#),
			format!(r#"{{"game_id":"game","leaderboards":[{board},{board},{board}]}}"#),
			"{}".into(),
		] {
			assert!(decode_list_response(response(200, &body).await, "game").await.is_err());
		}
		let body = format!(r#"{{"game_id":"game","leaderboards":[{board}]}}"#);
		assert!(decode_list_response(response(200, &body).await, "game").await.is_ok());
	}

	#[tokio::test]
	async fn proxy_preserves_server_status_and_json_or_plain_error_reason() {
		for status in [400, 422, 500] {
			let error = server_error(response(status, r#"{"message":"storage or validation failed"}"#).await, "Failed").await;
			let (code, Json(body)) = proxy_request_error(error);
			assert_eq!(code.as_u16(), status);
			assert_eq!(body.message, "storage or validation failed");
		}
		let error = server_error(response(422, "Invalid score").await, "Failed").await;
		assert_eq!(error.message, "Invalid score");
		let error = server_error(response(500, "").await, "Failed").await;
		assert!(error.message.contains("HTTP 500"));
	}

	#[tokio::test]
	async fn proxy_rejects_malformed_success_instead_of_claiming_submission_succeeded() {
		for body in ["{}", r#"{"ok":false,"rank":1}"#, r#"{"ok":true,"rank":0}"#, "not-json"] {
			assert!(decode_submission_response(response(200, body).await).await.is_err());
		}
		assert_eq!(decode_submission_response(response(200, r#"{"ok":true,"rank":1}"#).await).await.unwrap().rank, 1);
	}

	#[test]
	fn score_proxy_preserves_exact_text_and_accepts_old_integers() {
		let submission: ScoreSubmission = serde_json::from_str(r#"{"player_name":"A","score":"1.234567890123456789e1000"}"#).unwrap();
		assert_eq!(submission.score, "1.234567890123456789e1000");
		let old: ScoreSubmission = serde_json::from_str(r#"{"player_name":"A","score":9223372036854775807}"#).unwrap();
		assert_eq!(old.score, "9223372036854775807");
		let encoded = serde_json::to_value(submission).unwrap();
		assert!(encoded["score"].is_string());
		assert!(serde_json::from_str::<ScoreSubmission>(r#"{"player_name":"A","score":100000000000000000000000000001}"#).is_err());
	}

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
