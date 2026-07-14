use std::fs;
use std::env;
use std::path::PathBuf;
use tonic::transport::{Endpoint, Channel};
use gamelauncher::game_service_client::GameServiceClient;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClientConfig {
	pub server_url: String,
	pub games_path: String,
	pub animations_enabled: bool,
}

impl Default for ClientConfig {
	fn default() -> Self {
		Self {
			server_url: "http://[::1]:50050".to_string(),
			games_path: "".to_string(),
			animations_enabled: true,
		}
	}
}

pub fn normalize_server_url(url: &str) -> String {
	let u = url.trim();
	if u.is_empty() {
		return "http://[::1]:50050".to_string();
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
		result.push_str(":50050");
	}
	result
}

pub fn get_config() -> ClientConfig {
	if let Ok(base) = get_base_path() {
		let config_file = base.join("config.json");
		if let Ok(content) = fs::read_to_string(&config_file) {
			if let Ok(mut cfg) = serde_json::from_str::<ClientConfig>(&content) {
				cfg.server_url = normalize_server_url(&cfg.server_url);
				return cfg;
			}
		}
	}
	ClientConfig::default()
}

pub fn save_config(mut cfg: ClientConfig) -> Result<(), String> {
	let base = get_base_path()?;
	let config_file = base.join("config.json");
	cfg.server_url = normalize_server_url(&cfg.server_url);
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
	let data_path = if !cfg.games_path.trim().is_empty() {
		PathBuf::from(cfg.games_path.trim())
	} else {
		let mut base = get_base_path()?;
		base.push("games");
		base
	};

	fs::create_dir_all(&data_path).map_err(|e| e.to_string())?;

	Ok(data_path)
}

pub fn connect_and_get_client(url: String) -> GameServiceClient<Channel> 
{
	let url = normalize_server_url(&url);
	let endpoint = match Endpoint::from_shared(url) {
		Ok(ep) => ep,
		Err(_) => Endpoint::from_static("http://[::1]:50050"),
	};
	let channel = endpoint.connect_lazy();

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
	#[serde(rename = "latestUpdate", alias = "lastUpdate", alias = "latest_update", default, deserialize_with = "null_to_default")]
	pub latest_update: String,
	#[serde(default, deserialize_with = "null_to_default")]
	pub description: String,
	
	#[serde(flatten, skip_serializing)]
	#[allow(dead_code)]
	pub extra: Map<String, Value>,
}
