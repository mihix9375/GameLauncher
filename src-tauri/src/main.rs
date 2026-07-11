// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod env;
mod commands;

#[tokio::main]
async fn main()
{
	let url = "http://[::1]:50050".to_string();
	let client = env::connect_and_get_client(url);

	tauri::Builder::default()
	.manage(client)
	.invoke_handler(tauri::generate_handler![
		commands::launch::launch,
		commands::refresh::refresh,
		commands::check_version::check_version,
		commands::download_game::download_game
	])
	.run(tauri::generate_context!())
	.expect("error while running tauri application");
}
