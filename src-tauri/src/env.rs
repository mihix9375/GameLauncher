use std::fs;
use std::env;
use std::path::PathBuf;
use tonic::transport::{Endpoint, Channel};
use gamelauncher::game_service_client::GameServiceClient;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub fn get_base_path() -> Result<PathBuf, String>
{
	let mut data_path = PathBuf::from(env::var("appdata").map_err(|e| e.to_string())?);
	data_path.push("gamelauncher");

	fs::create_dir_all(&data_path).map_err(|e| e.to_string())?;

	Ok(data_path)
}

pub fn get_games_path() -> Result<PathBuf, String>
{
	let mut data_path = get_base_path()?;
	data_path.push("games");

	fs::create_dir_all(&data_path).map_err(|e| e.to_string())?;

	Ok(data_path)
}

pub fn connect_and_get_client(url: String) -> GameServiceClient<Channel> 
{
	let channel = Endpoint::from_shared(url)
		.expect("Invalid URL.")
		.connect_lazy();

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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Meta
{
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
	pub extra: Map<String, Value>,
}
