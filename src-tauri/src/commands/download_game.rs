use crate::env::gamelauncher::DownloadRequest;
use std::path::{Component, Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tonic::Request;

static DOWNLOAD_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

struct UpdatePaths
{
	work_dir: PathBuf,
	archive: PathBuf,
	extracted: PathBuf,
	backup: PathBuf,
}

impl UpdatePaths
{
	fn new(games_path: &Path, game_id: &str) -> Self
	{
		let unique = format!(
			"{}-{}-{}",
			game_id,
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap_or_default()
				.as_nanos(),
		);
		let work_dir = games_path.join(".updates").join(&unique);
		Self {
			archive: work_dir.join("download.zip"),
			extracted: work_dir.join("extracted"),
			backup: games_path.join(".updates").join(format!("{game_id}.backup")),
			work_dir,
		}
	}
}

#[tauri::command]
pub async fn download_game(game_id: String, version: String) -> Result<(), String>
{
	let _download_guard = DOWNLOAD_LOCK.get_or_init(|| tokio::sync::Mutex::new(())).lock().await;
	let games_path = crate::env::get_games_path()?;
	let clean_id = crate::env::normalize_game_id(&game_id)?;
	let game_path = games_path.join(&clean_id);
	let paths = UpdatePaths::new(&games_path, &clean_id);
	recover_game_backup(&game_path, &paths.backup).await?;

	tokio::fs::create_dir_all(&paths.extracted)
		.await
		.map_err(|e| e.to_string())?;

	let result = download_and_install(&clean_id, &version, &game_path, &paths).await;
	let _ = tokio::fs::remove_dir_all(&paths.work_dir).await;
	if result.is_ok() {
		let _ = tokio::fs::remove_dir_all(&paths.backup).await;
	}
	result
}

async fn recover_game_backup(game_path: &Path, backup: &Path) -> Result<(), String>
{
	if !backup.exists() { return Ok(()); }
	if game_path.exists() {
		tokio::fs::remove_dir_all(backup).await.map_err(|e| e.to_string())?;
	} else {
		tokio::fs::rename(backup, game_path).await.map_err(|e| format!("中断された更新の復旧に失敗しました: {e}"))?;
	}
	Ok(())
}

async fn download_and_install(
	game_id: &str,
	version: &str,
	game_path: &Path,
	paths: &UpdatePaths,
) -> Result<(), String>
{
	let url = crate::env::get_config().server_url;
	let mut client = crate::env::connect_and_get_client(url);
	let request = Request::new(DownloadRequest {
		game_id: game_id.to_string(),
		version: version.to_string(),
	});
	let response = client.download_game(request).await.map_err(|e| e.to_string())?;
	let mut stream = response.into_inner();

	let file = tokio::fs::OpenOptions::new()
		.create_new(true)
		.write(true)
		.open(&paths.archive)
		.await
		.map_err(|e| e.to_string())?;
	let mut writer = tokio::io::BufWriter::with_capacity(1024 * 1024 * 8, file);
	let mut expected_index = 0;
	let mut received_data = false;
	while let Some(chunk) = stream.message().await.map_err(|e| e.to_string())?
	{
		if chunk.index != expected_index {
			return Err("ダウンロードデータの順序が不正です".to_string());
		}
		expected_index += 1;
		received_data = true;
		writer.write_all(&chunk.data).await.map_err(|e| e.to_string())?;
	}
	if !received_data {
		return Err("ダウンロードデータが空です".to_string());
	}
	writer.flush().await.map_err(|e| e.to_string())?;
	drop(writer);

	let archive = paths.archive.clone();
	let extracted = paths.extracted.clone();
	tokio::task::spawn_blocking(move || extract_archive(&archive, &extracted))
		.await
		.map_err(|e| e.to_string())??;
	validate_staged_game(&paths.extracted, game_id, version)?;

	install_atomically(game_path, &paths.extracted, &paths.backup).await
}

fn validate_staged_game(directory: &Path, game_id: &str, requested_version: &str) -> Result<(), String>
{
	let content = std::fs::read_to_string(directory.join("meta.json"))
		.map_err(|_| "ZIPのルートにmeta.jsonがありません".to_string())?;
	let meta: crate::env::Meta = serde_json::from_str(&content)
		.map_err(|e| format!("meta.jsonを解析できません: {e}"))?;
	let meta_id = if meta.id.is_empty() { game_id.to_string() } else { crate::env::normalize_game_id(&meta.id)? };
	if meta_id != game_id {
		return Err("meta.jsonのゲームIDが要求と一致しません".to_string());
	}
	if meta.version.trim() != requested_version.trim() {
		return Err(format!("meta.jsonのバージョンが要求と一致しません: {}", meta.version));
	}
	let executable = crate::env::safe_game_relative_path(directory, &meta.game)?;
	if !executable.is_file() {
		return Err(format!("実行ファイルがZIP内にありません: {}", meta.game));
	}
	Ok(())
}

fn extract_archive(zip_path: &Path, destination: &Path) -> Result<(), String>
{
	use std::fs::File;
	use zip::ZipArchive;

	let zip_file = File::open(zip_path).map_err(|e| e.to_string())?;
	let mut archive = ZipArchive::new(zip_file).map_err(|e| e.to_string())?;
	for index in 0..archive.len()
	{
		let mut file = archive.by_index(index).map_err(|e| e.to_string())?;
		let raw_name = file.name_raw();
		let decoded_name = if let Ok(value) = std::str::from_utf8(raw_name)
		{
			value.to_string()
		}
		else
		{
			let (value, _, _) = encoding_rs::SHIFT_JIS.decode(raw_name);
			value.into_owned()
		};
		let relative = Path::new(&decoded_name);
		if decoded_name.trim().is_empty()
			|| decoded_name.contains(':')
			|| relative.components().any(|part| !matches!(part, Component::Normal(_)))
		{
			return Err(format!("ZIP内に不正なパスがあります: {decoded_name}"));
		}
		if file.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000)
		{
			return Err(format!("ZIP内のシンボリックリンクは使用できません: {decoded_name}"));
		}

		let outpath = destination.join(relative);
		if file.is_dir() || decoded_name.ends_with('/') || decoded_name.ends_with('\\')
		{
			std::fs::create_dir_all(&outpath).map_err(|e| e.to_string())?;
		}
		else
		{
			if let Some(parent) = outpath.parent() {
				std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
			}
			let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
			std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
		}
	}
	Ok(())
}

async fn install_atomically(game_path: &Path, extracted: &Path, backup: &Path) -> Result<(), String>
{
	let had_existing = game_path.exists();
	if had_existing {
		tokio::fs::rename(game_path, backup).await.map_err(|e| format!("旧バージョンの退避に失敗しました: {e}"))?;
	}

	if let Err(error) = tokio::fs::rename(extracted, game_path).await
	{
		if had_existing {
			let _ = tokio::fs::rename(backup, game_path).await;
		}
		return Err(format!("新バージョンの適用に失敗しました: {error}"));
	}

	if had_existing {
		tokio::fs::remove_dir_all(backup).await.map_err(|e| format!("旧バージョンの削除に失敗しました: {e}"))?;
	}
	Ok(())
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[tokio::test]
	async fn replaces_existing_game_and_removes_backup()
	{
		let root = std::env::temp_dir().join(format!(
			"gamelauncher-test-{}-{}",
			std::process::id(),
			std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
		));
		let game = root.join("game");
		let extracted = root.join("extracted");
		let backup = root.join("backup");
		std::fs::create_dir_all(&game).unwrap();
		std::fs::create_dir_all(&extracted).unwrap();
		std::fs::write(game.join("old.txt"), "old").unwrap();
		std::fs::write(extracted.join("new.txt"), "new").unwrap();

		install_atomically(&game, &extracted, &backup).await.unwrap();

		assert!(game.join("new.txt").is_file());
		assert!(!game.join("old.txt").exists());
		assert!(!backup.exists());
		let _ = std::fs::remove_dir_all(root);
	}
}
