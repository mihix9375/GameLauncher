use std::path::Path;

use tauri::AppHandle;

pub async fn apply_server_deletion(app_handle: &AppHandle, game_id: &str) -> Result<bool, String>
{
	let clean_id = crate::env::normalize_game_id(game_id)?;
	let _storage_guard = super::download_game::GAME_STORAGE_LOCK
		.get_or_init(|| tokio::sync::Mutex::new(()))
		.lock()
		.await;
	let games_path = crate::env::get_games_path()?;
	let game_path = games_path.join(&clean_id);
	if !game_path.exists() { return Ok(false); }

	super::launch::terminate_game_if_running(app_handle, &clean_id)?;
	remove_game_directory(&game_path).await?;
	let backup = games_path.join(".updates").join(format!("{clean_id}.backup"));
	if backup.exists()
	{
		tokio::fs::remove_dir_all(backup).await
			.map_err(|error| format!("更新バックアップを削除できません: {error}"))?;
	}
	Ok(true)
}

async fn remove_game_directory(path: &Path) -> Result<(), String>
{
	tokio::fs::remove_dir_all(path).await
		.map_err(|error| format!("ゲームを削除できません ({}): {error}", path.display()))
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[tokio::test]
	async fn removes_only_the_requested_game_directory()
	{
		let root = std::env::temp_dir().join(format!(
			"gamelauncher-delete-{}-{}",
			std::process::id(),
			std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
		));
		let target = root.join("target");
		let other = root.join("other");
		std::fs::create_dir_all(&target).unwrap();
		std::fs::create_dir_all(&other).unwrap();
		std::fs::write(target.join("game.exe"), b"test").unwrap();

		remove_game_directory(&target).await.unwrap();
		assert!(!target.exists());
		assert!(other.exists());
		let _ = std::fs::remove_dir_all(root);
	}
}
