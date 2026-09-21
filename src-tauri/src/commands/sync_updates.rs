#[tauri::command]
pub async fn sync_updates
(
	app_handle: tauri::AppHandle,
) -> Result<(), String>
{
	crate::wait_update::restart_wait_update(app_handle).await;
	Ok(())
}
