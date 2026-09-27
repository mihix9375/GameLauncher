use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use gamelauncher::game_service_client::GameServiceClient;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tonic::transport::{Channel, Endpoint};

const DEFAULT_SERVER_URL: &str = "http://[::1]:50050";
const DEFAULT_LEADERBOARD_URL: &str = "http://127.0.0.1:50052";

static GRPC_CHANNEL: std::sync::OnceLock<tokio::sync::Mutex<Option<(String, Channel)>>> =
	std::sync::OnceLock::new();

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientConfig {
	pub server_url: String,
	#[serde(default = "default_leaderboard_url")]
	pub leaderboard_url: String,
	pub games_path: String,
	pub animations_enabled: bool,
}

impl Default for ClientConfig {
	fn default() -> Self {
		Self {
			server_url: DEFAULT_SERVER_URL.to_string(),
			leaderboard_url: default_leaderboard_url(),
			games_path: "".to_string(),
			animations_enabled: true,
		}
	}
}

fn default_leaderboard_url() -> String {
	DEFAULT_LEADERBOARD_URL.to_string()
}

pub fn normalize_server_url(url: &str) -> String {
	normalize_http_url(url, DEFAULT_SERVER_URL, 50050)
}

pub fn normalize_leaderboard_url(url: &str) -> String {
	normalize_http_url(url, DEFAULT_LEADERBOARD_URL, 50052)
}

fn normalize_http_url(url: &str, default_url: &str, default_port: u16) -> String {
	let u = url.trim();
	if u.is_empty() {
		return default_url.to_string();
	}
	let scheme_removed = if let Some(s) = u.strip_prefix("http://") {
		s
	} else if let Some(s) = u.strip_prefix("https://") {
		s
	} else {
		u
	};
	let has_port = if scheme_removed.starts_with('[') {
		if let Some(bracket_end) = scheme_removed.find(']') {
			scheme_removed[bracket_end..].contains(':')
		} else {
			false
		}
	} else {
		scheme_removed.contains(':')
	};

	let mut result = if u.starts_with("http://") || u.starts_with("https://") {
		u.to_string()
	} else {
		format!("http://{}", u)
	};

	if !has_port {
		result.push_str(&format!(":{default_port}"));
	}
	result
}

pub fn get_config() -> ClientConfig
{
	if let Ok(base) = get_base_path()
	{
		let config_file = base.join("config.json");
		if let Ok(content) = fs::read_to_string(&config_file)
		{
			if let Ok(mut cfg) = serde_json::from_str::<ClientConfig>(&content)
			{
				cfg.server_url = normalize_server_url(&cfg.server_url);
				cfg.leaderboard_url = normalize_leaderboard_url(&cfg.leaderboard_url);
				return cfg;
			}
		}
	}
	ClientConfig::default()
}

pub fn save_config(mut cfg: ClientConfig) -> Result<(), String>
{
	let base = get_base_path()?;
	let config_file = base.join("config.json");
	cfg.server_url = normalize_server_url(&cfg.server_url);
	cfg.leaderboard_url = normalize_leaderboard_url(&cfg.leaderboard_url);
	let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
	fs::write(&config_file, json).map_err(|e| e.to_string())?;
	Ok(())
}

pub fn get_base_path() -> Result<PathBuf, String>
{
	let mut data_path = PathBuf::from(env::var("appdata").map_err(|e| e.to_string())?);
	data_path.push("gamelauncher");

	fs::create_dir_all(&data_path).map_err(|e| e.to_string())?;

	Ok(data_path)
}

pub fn get_games_path() -> Result<PathBuf, String>
{
	let cfg = get_config();
	let data_path = if !cfg.games_path.trim().is_empty()
	{
		PathBuf::from(cfg.games_path.trim())
	}
	else
	{
		let mut base = get_base_path()?;
		base.push("games");
		base
	};

	fs::create_dir_all(&data_path).map_err(|e| e.to_string())?;

	Ok(data_path)
}

pub fn normalize_game_id(game_id: &str) -> Result<String, String>
{
	let value = game_id.trim();
	let value = if value.to_ascii_lowercase().ends_with(".exe")
	{
		&value[..value.len() - 4]
	}
	else
	{
		value
	};

	if value.is_empty()
		|| value == "."
		|| value == ".."
		|| value.contains(['/', '\\', ':'])
		|| !matches!(Path::new(value).components().collect::<Vec<_>>().as_slice(), [Component::Normal(_)])
	{
		return Err("不正なゲームIDです".to_string());
	}

	Ok(value.to_string())
}

pub fn safe_game_relative_path(base: &Path, relative: &str) -> Result<PathBuf, String>
{
	let path = Path::new(relative);
	if relative.trim().is_empty()
		|| relative.contains(':')
		|| path.components().any(|component| !matches!(component, Component::Normal(_)))
	{
		return Err("不正な実行ファイルパスです".to_string());
	}

	Ok(base.join(path))
}

pub async fn connect_and_get_client(url: String) -> GameServiceClient<Channel>
{
	let url = normalize_server_url(&url);
	let cache = GRPC_CHANNEL.get_or_init(|| tokio::sync::Mutex::new(None));
	let mut cached = cache.lock().await;
	if let Some((cached_url, channel)) = cached.as_ref()
	{
		if cached_url == &url
		{
			return GameServiceClient::new(channel.clone());
		}
	}

	let endpoint = match Endpoint::from_shared(url.clone())
	{
		Ok(ep) => ep,
		Err(_) => Endpoint::from_static("http://[::1]:50050"),
	};
	let configured_endpoint = endpoint
		.initial_stream_window_size(Some(1024 * 1024 * 64))
		.initial_connection_window_size(Some(1024 * 1024 * 256))
		.http2_adaptive_window(false)
		.tcp_nodelay(true);
	let channel = configured_endpoint.connect_lazy();
	*cached = Some((url, channel.clone()));
	GameServiceClient::new(channel)
}

pub mod gamelauncher {
	tonic::include_proto!("gamelauncher");
}

fn null_to_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
	D: serde::Deserializer<'de>,
	T: Default + Deserialize<'de>,
{
	let opt = Option::<T>::deserialize(deserializer)?;
	Ok(opt.unwrap_or_default())
}

fn date_to_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let value = Option::<Value>::deserialize(deserializer)?;
	let text = match value
	{
		Some(Value::String(value)) => value,
		Some(Value::Number(value)) => value.to_string(),
		Some(Value::Bool(value)) => value.to_string(),
		_ => String::new(),
	};
	Ok(normalize_meta_date(&text))
}

/// Accepts common date spellings used by game authors and returns one display format.
/// Unknown or invalid values are kept as-is instead of being discarded.
pub fn normalize_meta_date(value: &str) -> String
{
	let original = value.trim();
	if original.is_empty()
	{
		return String::new();
	}

	let date_part = original.split(['T', ' ']).next().unwrap_or(original);
	let normalized = date_part
		.replace('年', "/")
		.replace('月', "/")
		.replace('日', "")
		.replace(['-', '.'], "/");
	let parts: Vec<&str> = normalized.split('/').filter(|part| !part.is_empty()).collect();
	let parsed = if parts.len() == 3
	{
		Some((parts[0], parts[1], parts[2]))
	}
	else if normalized.len() == 8 && normalized.bytes().all(|byte| byte.is_ascii_digit())
	{
		Some((&normalized[0..4], &normalized[4..6], &normalized[6..8]))
	}
	else
	{
		None
	};

	let Some((year, month, day)) = parsed else { return original.to_string(); };
	let (Ok(year), Ok(month), Ok(day)) = (year.parse::<u32>(), month.parse::<u32>(), day.parse::<u32>()) else
	{
		return original.to_string();
	};
	let leap_year = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
	let max_day = match month
	{
		1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
		4 | 6 | 9 | 11 => 30,
		2 if leap_year => 29,
		2 => 28,
		_ => return original.to_string(),
	};
	if !(1..=max_day).contains(&day)
	{
		return original.to_string();
	}
	format!("{year:04}/{month:02}/{day:02}")
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct Meta
{
	#[serde(default, deserialize_with = "null_to_default")]
	pub id: String,
	#[serde(default, deserialize_with = "null_to_default")]
	pub title: String,
	#[serde(default, deserialize_with = "null_to_default")]
	pub author: String,
	#[serde(rename = "titleImage", alias = "title_image", alias = "TitleImage", alias = "image", alias = "imgName", default, deserialize_with = "null_to_default")]
	pub title_image: String,
	#[serde(default, deserialize_with = "null_to_default")]
	pub tags: Vec<String>,
	#[serde(rename = "game", alias = "exeName", alias = "exe", alias = "gameExe", default, deserialize_with = "null_to_default")]
	pub game: String,
	#[serde(default, deserialize_with = "null_to_default")]
	pub version: String,
	#[serde(rename = "latestUpdate", alias = "lastUpdate", alias = "latest_update", default, deserialize_with = "date_to_string")]
	pub latest_update: String,
	#[serde(default, deserialize_with = "null_to_default")]
	pub description: String,
	
	#[serde(flatten, skip_serializing)]
	#[allow(dead_code)]
	pub extra: Map<String, Value>,
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn normalizes_common_meta_date_spellings()
	{
		for value in ["2026/9/6", "2026-09-06", "2026.9.6", "2026年9月6日", "20260906", "2026-09-06T12:34:56Z"]
		{
			assert_eq!(normalize_meta_date(value), "2026/09/06");
		}
	}

	#[test]
	fn preserves_unknown_or_invalid_meta_dates()
	{
		assert_eq!(normalize_meta_date("秋ごろ"), "秋ごろ");
		assert_eq!(normalize_meta_date("2026-02-30"), "2026-02-30");
	}

	#[test]
	fn game_id_accepts_a_single_file_name()
	{
		assert_eq!(normalize_game_id("テスト Game.exe").unwrap(), "テスト Game");
	}

	#[test]
	fn game_id_rejects_paths()
	{
		for value in ["../game", "folder/game", "folder\\game", "C:\\game", ""]
		{
			assert!(normalize_game_id(value).is_err(), "{value}");
		}
	}

	#[test]
	fn executable_path_must_stay_relative()
	{
		let base = Path::new("games").join("sample");
		assert!(safe_game_relative_path(&base, "bin/game.exe").is_ok());
		assert!(safe_game_relative_path(&base, "../game.exe").is_err());
		assert!(safe_game_relative_path(&base, "C:\\game.exe").is_err());
	}
}
