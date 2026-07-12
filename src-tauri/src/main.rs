// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod env;
mod commands;
mod wait_update;

#[tokio::main]
async fn main()
{
	let url = env::get_config().server_url;
	let client = env::connect_and_get_client(url);

	tauri::Builder::default()
	.manage(client.clone())
	.setup(move |app| {
		let handle = app.handle().clone();
		tauri::async_runtime::spawn(async move {
			wait_update::restart_wait_update(handle).await;
		});
		Ok(())
	})
	.invoke_handler(tauri::generate_handler![
		commands::launch::launch,
		commands::refresh::refresh,
		commands::check_version::check_version,
		commands::download_game::download_game,
		commands::sync_updates::sync_updates,
		commands::settings::get_client_config,
		commands::settings::save_client_config
	])
	.run(tauri::generate_context!())
	.expect("error while running tauri application");
}
