use std::sync::OnceLock;
use std::time::Duration;
use reqwest::{Client, Url};

/// Share the HTTP/2 connection pool between ranking and metadata requests.
pub fn client() -> Result<&'static Client, String>
{
	static CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();
	CLIENT.get_or_init(|| Client::builder()
		.timeout(Duration::from_secs(5))
		.build()
		.map_err(|error| format!("Server用HTTPクライアントを作成できません: {error}")))
		.as_ref().map_err(Clone::clone)
}

pub fn game_endpoint(game_id: &str, resource: &[&str]) -> Result<Url, String>
{
	let config = crate::env::get_config();
	endpoint(&crate::env::normalize_leaderboard_url(&config.leaderboard_url), game_id, resource)
}

fn endpoint(base: &str, game_id: &str, resource: &[&str]) -> Result<Url, String>
{
	let mut url = Url::parse(base).map_err(|error| format!("Server APIのURLが不正です: {error}"))?;
	url.path_segments_mut().map_err(|_| "Server APIのURLが不正です".to_string())?
		.pop_if_empty().extend(["v1", "games", game_id]).extend(resource.iter().copied());
	Ok(url)
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn endpoints_preserve_prefixes_and_encode_ids_as_single_segments()
	{
		let metadata = endpoint("http://127.0.0.1:50052/api/", "Game Name", &["metadata"]).unwrap();
		assert_eq!(metadata.as_str(), "http://127.0.0.1:50052/api/v1/games/Game%20Name/metadata");
		let ranking = endpoint("http://[::1]:50052", "a/b", &["leaderboards", "0", "scores"]).unwrap();
		assert_eq!(ranking.as_str(), "http://[::1]:50052/v1/games/a%2Fb/leaderboards/0/scores");
	}
}
