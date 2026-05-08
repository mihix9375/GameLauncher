// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn deply_commands()
{
	tauri::Builder::default()
	.invoke_handler(tauri::generate_handler![
		commands::launch::launch,
		commands::reflesh::reflesh
	])
	.run(tauri::generate_context!())
	.expect("error while running tauri application");
}

fn main()
{
	deply_commands()
}
