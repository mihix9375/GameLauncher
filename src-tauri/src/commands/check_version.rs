use serde::Serialize;
use tonic::Request;
use crate::env::gamelauncher::VersionRequest;

#[derive(Serialize)]
pub struct SerializedVersionResponse
{
	pub latest_version: String,
	pub is_update_available: bool,
}

#[tauri::command]
pub async fn check_version(
	version: String,
	game_id: String,
) -> Result<SerializedVersionResponse, String> {
	let url = crate::env::get_config().server_url;
	let mut client = crate::env::connect_and_get_client(url);
	let clean_id = game_id.trim_end_matches(".exe");

	let request = Request::new(VersionRequest {
		 game_id: clean_id.to_string(),
		 current_version: version,
	});

	let response = client.check_version(request)
		.await
		.map_err(|e| e.to_string())?;

	let data = response.into_inner();

	Ok(SerializedVersionResponse {
		latest_version: data.latest_version,
		is_update_available: data.is_update_available,
	})
}
