// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod env;
mod description;
mod server_api;
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
		// 常駐する非表示オーバーレイを含む初期化が完了してからメイン窓を見せる。
		// 起動途中の最小化でメイン窓だけが失われる競合を防ぐ。
		let main_window = app
			.get_webview_window("main")
			.ok_or_else(|| std::io::Error::other("メインウィンドウが見つかりません"))?;
		main_window.show().map_err(std::io::Error::other)?;
		main_window.set_focus().map_err(std::io::Error::other)?;
		Ok(())
	})
	.on_window_event(|window, event| {
		if window.label() == "main"
		{
			match event {
				tauri::WindowEvent::CloseRequested { api, .. } => {
					api.prevent_close();
					commands::launch::begin_shutdown(window.app_handle().clone());
				}
				// メイン窓だけが予期せず破棄されても、非表示のオーバーレイ窓を
				// 残してプロセスを常駐させない。
				tauri::WindowEvent::Destroyed => {
					commands::launch::begin_shutdown(window.app_handle().clone());
				}
				_ => {}
			}
		}
	})
	.invoke_handler(tauri::generate_handler![
		commands::launch::launch,
		commands::launch::is_game_running,
		commands::launch::get_running_game_id,
		commands::launch::request_close_game,
		commands::launch::close_game,
		commands::launch::cancel_close_game,
		commands::refresh::refresh,
		commands::catalog::get_game_metadata,
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
