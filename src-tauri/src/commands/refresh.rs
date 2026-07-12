use crate::env::{self, Meta};
use std::fs;
use base64::Engine;

#[tauri::command]
pub fn refresh() -> Vec<Meta>
{
	let mut metas = Vec::new();
	if let Ok(games_path) = env::get_games_path() {
		// まず .exe が付いた不要なフォルダをクリーンアップする
		if let Ok(entries) = fs::read_dir(&games_path) {
			for entry in entries.flatten() {
				if entry.path().is_dir() {
					let name = entry.file_name().to_string_lossy().to_string();
					if name.ends_with(".exe") {
						let clean_name = name.trim_end_matches(".exe");
						let clean_path = games_path.join(clean_name);
						// クリーンなフォルダが既に存在するか、.exeフォルダに meta.jsonが無い場合は不要なので削除
						if clean_path.exists() || !entry.path().join("meta.json").exists() {
							let _ = fs::remove_dir_all(entry.path());
						}
					}
				}
			}
		}

		if let Ok(entries) = fs::read_dir(&games_path) {
			for entry in entries.flatten() {
				if entry.path().is_dir() {
					let name = entry.file_name().to_string_lossy().to_string();
					if name.ends_with(".exe") {
						continue;
					}
					let meta_path = entry.path().join("meta.json");
					if let Ok(content) = fs::read_to_string(&meta_path) {
						if let Ok(mut meta) = serde_json::from_str::<Meta>(&content) {
							if meta.id.is_empty() {
								meta.id = name.clone();
							}
							meta.id = meta.id.trim_end_matches(".exe").to_string();
							if meta.game.is_empty() {
								meta.game = format!("{}.exe", meta.id);
							}
							if !meta.title_image.is_empty() && !meta.title_image.starts_with("data:") && !meta.title_image.starts_with("http") {
								let img_path = entry.path().join(&meta.title_image);
								if let Ok(bytes) = fs::read(&img_path) {
									let ext = img_path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
									let mime = match ext.as_str() {
										"jpg" | "jpeg" => "image/jpeg",
										"gif" => "image/gif",
										"webp" => "image/webp",
										_ => "image/png",
									};
									let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
									meta.title_image = format!("data:{};base64,{}", mime, encoded);
								}
							}
							metas.push(meta);
						}
					}
				}
			}
		}
	}
	metas
}

