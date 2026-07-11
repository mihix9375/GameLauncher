use crate::env::{self, Meta};
use std::fs;

#[tauri::command]
pub fn refresh() -> Vec<Meta>
{
	let mut metas = Vec::new();
	if let Ok(games_path) = env::get_games_path() {
		if let Ok(entries) = fs::read_dir(games_path) {
			for entry in entries.flatten() {
				if entry.path().is_dir() {
					let meta_path = entry.path().join("meta.json");
					if let Ok(content) = fs::read_to_string(&meta_path) {
						if let Ok(meta) = serde_json::from_str::<Meta>(&content) {
							metas.push(meta);
						}
					}
				}
			}
		}
	}
	metas
}
