use std::process::Command;
use crate::env;

#[tauri::command]
pub fn launch(game_id: String) -> Result<(), String>
{
	let games_path = env::get_games_path()?;
	let clean_id = game_id.trim_end_matches(".exe");
	let game_dir = games_path.join(clean_id);

	let meta_path = game_dir.join("meta.json");
	let exe_name = if meta_path.exists() {
		if let Ok(content) = std::fs::read_to_string(&meta_path) {
			if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
				json["game"].as_str().or(json["exeName"].as_str()).map(|s| s.to_string())
			} else {
				None
			}
		} else {
			None
		}
	} else {
		None
	};

	let mut exe_path = if let Some(exe) = exe_name {
		game_dir.join(exe)
	} else {
		game_dir.join(format!("{}.exe", clean_id))
	};

	if !exe_path.exists() {
		let fallback = game_dir.join(format!("{}.exe", clean_id));
		if fallback.exists() {
			exe_path = fallback;
		} else if let Ok(entries) = std::fs::read_dir(&game_dir) {
			for entry in entries.flatten() {
				if let Some(ext) = entry.path().extension() {
					if ext.to_string_lossy().eq_ignore_ascii_case("exe") && !entry.file_name().to_string_lossy().contains("UnityCrashHandler") {
						exe_path = entry.path();
						break;
					}
				}
			}
		}
	}

	if !exe_path.exists() {
		return Err(format!("実行ファイルが見つかりません: {}", exe_path.display()));
	}

	Command::new(&exe_path)
		.current_dir(&game_dir)
		.spawn()
		.map_err(|e| format!("起動に失敗しました ({}): {}", exe_path.display(), e))?;

	Ok(())
}
