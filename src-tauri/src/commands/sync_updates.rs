use tonic::Request;
use crate::env::gamelauncher::{
	Identificial
};
use tauri::Emitter;

#[tauri::command]
pub async fn sync_updates
(
	app_handle: tauri::AppHandle,
) -> Result<(), String>
{
	let url = crate::env::get_config().server_url;
	let mut client = crate::env::connect_and_get_client(url);
	let current_ip = crate::wait_update::get_current_ip();

	let request = Request::new(Identificial {
		ip_addr: current_ip
	});

	if let Ok(response) = client.wait_update(request).await {
		let mut stream = response.into_inner();

		while let Ok(Ok(Some(notice))) = tokio::time::timeout(
			std::time::Duration::from_millis(1500),
			stream.message(),
		).await {
			let _ = app_handle.emit("update_notice", serde_json::json!({
				"game_id": notice.game_id,
				"version": notice.version,
			}));
		}
	}

	Ok(())
}
