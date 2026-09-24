// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod env;
mod commands;
mod wait_update;

use tauri::Manager;

#[tokio::main]
async fn main()
{
	tauri::Builder::default()
	.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
		if let Some(window) = app.get_webview_window("main")
		{
			let _ = window.show();
			let _ = window.unminimize();
			let _ = window.set_focus();
		}
	}))
	.manage(commands::launch::GameProcessState::default())
	.setup(|app| {
		commands::launch::prepare_overlay(app.handle())
			.map_err(std::io::Error::other)?;
		let process_state = app.state::<commands::launch::GameProcessState>().inner().clone();
		tauri::async_runtime::spawn(commands::leaderboards::serve_local_api(process_state));
		Ok(())
	})
	.on_window_event(|window, event| {
		if window.label() == "main"
		{
			if let tauri::WindowEvent::CloseRequested { api, .. } = event
			{
				api.prevent_close();
				commands::launch::begin_shutdown(window.app_handle().clone());
			}
		}
	})
	.invoke_handler(tauri::generate_handler![
		commands::launch::launch,
		commands::launch::is_game_running,
		commands::launch::close_game,
		commands::refresh::refresh,
		commands::check_version::check_version,
		commands::download_game::download_game,
		commands::sync_updates::sync_updates,
		commands::settings::get_client_config,
		commands::settings::save_client_config,
		commands::comments::list_comments,
		commands::comments::add_comment,
		commands::leaderboards::get_leaderboards
	])
	.run(tauri::generate_context!())
	.expect("error while running tauri application");
}
