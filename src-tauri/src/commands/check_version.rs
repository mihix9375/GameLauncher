use serde::Serialize;
use tonic::{Request, transport::Channel};
use crate::env::gamelauncher::VersionRequest;
use crate::env::gamelauncher::game_service_client::GameServiceClient;

#[derive(Serialize)]
pub struct SerializedVersionResponse
{
	pub latest_version: String,
	pub is_update_available: bool,
}

#[tauri::command]
pub async fn check_version(
	version: String,
	id: String,
	client_state: tauri::State<'_, GameServiceClient<Channel>>,
) -> Result<SerializedVersionResponse, String> {
	let mut client = client_state.inner().clone();

	let request = Request::new(VersionRequest {
		 game_id: id,
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
