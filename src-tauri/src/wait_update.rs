use crate::env::gamelauncher::{Identificial, UpdateAction};
use tonic::Request;
use tauri::Emitter;

pub fn get_current_ip() -> String {
	let mut current_ip = "127.0.0.1".to_string();
	for ip in get_ip_list()
	{
		if ip.contains("192.168")
		{
			current_ip = ip;
			break;
		}
	}
	current_ip
}

static WAIT_UPDATE_HANDLE: std::sync::OnceLock<tokio::sync::Mutex<Option<tauri::async_runtime::JoinHandle<()>>>> = std::sync::OnceLock::new();

pub async fn restart_wait_update(app_handle: tauri::AppHandle)
{
	let handle_mutex = WAIT_UPDATE_HANDLE.get_or_init(|| tokio::sync::Mutex::new(None));
	let mut guard = handle_mutex.lock().await;
	if let Some(old_handle) = guard.take() {
		old_handle.abort();
	}

	let new_handle = tauri::async_runtime::spawn(async move {
		loop {
			let url = crate::env::get_config().server_url;
			let mut client = crate::env::connect_and_get_client(url).await;
			let current_ip = get_current_ip();

			let request = Request::new(Identificial {
				ip_addr: current_ip,
				installed_game_ids: installed_game_ids(),
			});

			if let Ok(response) = client.wait_update(request).await {
				let mut stream = response.into_inner();
				while let Ok(Some(notice)) = stream.message().await {
					let Ok(game_id) = crate::env::normalize_game_id(&notice.game_id) else {
						continue;
					};
					if UpdateAction::try_from(notice.action).unwrap_or(UpdateAction::Upsert) == UpdateAction::Delete
					{
						let result = crate::commands::delete_game::apply_server_deletion(&app_handle, &game_id).await;
						let (deleted, error) = match result
						{
							Ok(deleted) => (deleted, None),
							Err(error) => (false, Some(error)),
						};
						let _ = app_handle.emit("game_delete_notice", serde_json::json!({
							"game_id": game_id,
							"deleted": deleted,
							"error": error,
						}));
						continue;
					}
					let _ = app_handle.emit("update_notice", serde_json::json!({
						"game_id": game_id,
						"version": notice.version,
					}));
				}
			}
			tokio::time::sleep(std::time::Duration::from_secs(3)).await;
		}
	});

	*guard = Some(new_handle);
}

fn installed_game_ids() -> Vec<String>
{
	let Ok(games_path) = crate::env::get_games_path() else { return Vec::new(); };
	let Ok(entries) = std::fs::read_dir(games_path) else { return Vec::new(); };
	let mut game_ids = entries.flatten()
		.filter(|entry| entry.path().is_dir() && entry.path().join("meta.json").is_file())
		.filter_map(|entry| crate::env::normalize_game_id(&entry.file_name().to_string_lossy()).ok())
		.collect::<Vec<_>>();
	game_ids.sort();
	game_ids.dedup();
	game_ids
}

fn get_ip_list() -> Vec<String> {
	use ipconfig;
	let mut ips: Vec<String> = Vec::new();
	match ipconfig::get_adapters() {
		Ok(adapters) => {
			for adapter in adapters {
				if adapter.oper_status() == ipconfig::OperStatus::IfOperStatusUp {
					for address in adapter.ip_addresses() {
						if !address.is_loopback() && address.is_ipv4() {
							ips.push(address.to_string());
						}
					}
				}
			}
		}
		_ => {}
	}
	ips
}
