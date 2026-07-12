use crate::env::ClientConfig;

#[tauri::command]
pub fn get_client_config() -> ClientConfig {
	crate::env::get_config()
}

#[tauri::command]
pub async fn save_client_config(app_handle: tauri::AppHandle, config: ClientConfig) -> Result<(), String> {
	crate::env::save_config(config)?;
	crate::wait_update::restart_wait_update(app_handle).await;
	Ok(())
}
