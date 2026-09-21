// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod env;
mod commands;
mod wait_update;

#[tokio::main]
async fn main()
{
	tauri::Builder::default()
	.manage(commands::launch::GameProcessState::default())
	.setup(|app| {
		commands::launch::prepare_overlay(app.handle())
			.map_err(std::io::Error::other)?;
		Ok(())
	})
	.invoke_handler(tauri::generate_handler![
		commands::launch::launch,
		commands::launch::close_game,
		commands::refresh::refresh,
		commands::check_version::check_version,
		commands::download_game::download_game,
		commands::sync_updates::sync_updates,
		commands::settings::get_client_config,
		commands::settings::save_client_config,
		commands::comments::list_comments,
		commands::comments::add_comment
	])
	.run(tauri::generate_context!())
	.expect("error while running tauri application");
}
