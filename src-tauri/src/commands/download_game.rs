use crate::env::gamelauncher::{DownloadRequest, GameFile, GameFilesRequest, GameManifest};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use tokio::io::AsyncWriteExt;
use tonic::Request;
use tauri::{AppHandle, Emitter};

use super::game_archive::extract_archive;

static GAME_STORAGE_LOCKS: std::sync::OnceLock<tokio::sync::Mutex<HashMap<String, std::sync::Arc<tokio::sync::Mutex<()>>>>> = std::sync::OnceLock::new();
const MANIFEST_FILE: &str = ".gamelauncher-manifest.json";
const DISK_WRITE_BUFFER_SIZE: usize = 8 * 1024 * 1024;
const FULL_ARCHIVE_WRITE_QUEUE_DEPTH: usize = 24;
const FILE_WRITE_QUEUE_DEPTH: usize = 4;
const PARALLEL_FILE_STREAMS: usize = 4;

pub(crate) async fn game_storage_lock(game_id: &str) -> std::sync::Arc<tokio::sync::Mutex<()>>
{
	let mut locks = GAME_STORAGE_LOCKS
		.get_or_init(|| tokio::sync::Mutex::new(HashMap::new()))
		.lock()
		.await;
	locks.entry(game_id.to_string())
		.or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
		.clone()
}

#[derive(Clone, Serialize)]
struct DownloadProgressEvent
{
	game_id: String,
	stage: String,
	received_bytes: u64,
	total_bytes: u64,
	error: Option<String>,
}

#[derive(Clone)]
struct DownloadProgress
{
	app_handle: AppHandle,
	game_id: String,
	received_bytes: std::sync::Arc<std::sync::atomic::AtomicU64>,
	total_bytes: u64,
}

impl DownloadProgress
{
	fn new(app_handle: &AppHandle, game_id: &str, total_bytes: u64) -> Self
	{
		Self {
			app_handle: app_handle.clone(),
			game_id: game_id.to_string(),
			received_bytes: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
			total_bytes,
		}
	}

	fn add_received(&self, bytes: u64)
	{
		let received = self.received_bytes.fetch_add(bytes, std::sync::atomic::Ordering::Relaxed)
			.saturating_add(bytes);
		self.emit("downloading", received, None);
	}

	fn stage(&self, stage: &str)
	{
		let received = self.received_bytes.load(std::sync::atomic::Ordering::Relaxed);
		self.emit(stage, received, None);
	}

	fn emit(&self, stage: &str, received_bytes: u64, error: Option<String>)
	{
		let _ = self.app_handle.emit("download_progress", DownloadProgressEvent {
			game_id: self.game_id.clone(),
			stage: stage.to_string(),
			received_bytes,
			total_bytes: self.total_bytes,
			error,
		});
	}
}

fn emit_download_state(app_handle: &AppHandle, game_id: &str, stage: &str, error: Option<String>)
{
	let _ = app_handle.emit("download_progress", DownloadProgressEvent {
		game_id: game_id.to_string(),
		stage: stage.to_string(),
		received_bytes: 0,
		total_bytes: 0,
		error,
	});
}

struct UpdatePaths
{
	work_dir: PathBuf,
	archive: PathBuf,
	extracted: PathBuf,
	backup: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstalledManifest
{
	version: String,
	files: Vec<InstalledFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InstalledFile
{
	path: String,
	size: u64,
	sha256: String,
}

impl UpdatePaths
{
	fn new(games_path: &Path, game_id: &str) -> Self
	{
		let unique = format!(
			"{:x}-{:x}",
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap_or_default()
				.as_nanos(),
		);
		let work_dir = games_path.join(".updates").join(&unique);
		Self {
			archive: work_dir.join("d.zip"),
			extracted: work_dir.join("x"),
			backup: games_path.join(".updates").join(format!("{game_id}.backup")),
			work_dir,
		}
	}
}

#[tauri::command]
pub async fn download_game(app_handle: AppHandle, game_id: String, version: String) -> Result<(), String>
{
	let clean_id = crate::env::normalize_game_id(&game_id)?;
	let storage_lock = game_storage_lock(&clean_id).await;
	let _download_guard = storage_lock.lock().await;
	let games_path = crate::env::get_games_path()?;
	emit_download_state(&app_handle, &clean_id, "preparing", None);
	let game_path = games_path.join(&clean_id);
	let paths = UpdatePaths::new(&games_path, &clean_id);
	recover_game_backup(&game_path, &paths.backup).await?;

	tokio::fs::create_dir_all(&paths.extracted)
		.await
		.map_err(|e| e.to_string())?;

	let result = download_and_install(&app_handle, &clean_id, &version, &game_path, &paths).await;
	let _ = tokio::fs::remove_dir_all(&paths.work_dir).await;
	if result.is_ok() {
		let _ = tokio::fs::remove_dir_all(&paths.backup).await;
		emit_download_state(&app_handle, &clean_id, "complete", None);
	} else if let Err(error) = &result {
		emit_download_state(&app_handle, &clean_id, "error", Some(error.clone()));
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
	app_handle: &AppHandle,
	game_id: &str,
	version: &str,
	game_path: &Path,
	paths: &UpdatePaths,
) -> Result<(), String>
{
	let url = crate::env::get_config().server_url;
	let mut client = crate::env::connect_and_get_client(url).await;
	let download_request = DownloadRequest {
		game_id: game_id.to_string(),
		version: version.to_string(),
	};
	let manifest = client.get_game_manifest(Request::new(download_request.clone()))
		.await
		.map_err(|error| error.message().to_string())?
		.into_inner();

	if game_path.is_dir()
	{
		install_differential_update(app_handle, &mut client, game_id, version, game_path, paths, &manifest).await?;
	}
	else
	{
		install_full_download(app_handle, game_id, &mut client, download_request, paths, &manifest).await?;
	}

	emit_download_state(app_handle, game_id, "installing", None);
	validate_staged_game(&paths.extracted, game_id, version)?;
	install_atomically(game_path, &paths.extracted, &paths.backup).await
}

async fn install_full_download(
	app_handle: &AppHandle,
	game_id: &str,
	client: &mut crate::env::gamelauncher::game_service_client::GameServiceClient<tonic::transport::Channel>,
	request: DownloadRequest,
	paths: &UpdatePaths,
	manifest: &GameManifest,
) -> Result<(), String>
{
	let progress = DownloadProgress::new(app_handle, game_id, manifest.archive_size);
	progress.stage("downloading");
	download_full_archive(client, request, paths, &progress).await?;
	progress.stage("extracting");
	let archive = paths.archive.clone();
	let extracted = paths.extracted.clone();
	tokio::task::spawn_blocking(move || {
		extract_archive(&archive, &extracted)?;
		flatten_single_game_root(&extracted)
	})
		.await
		.map_err(|error| error.to_string())??;
	verify_manifest(&paths.extracted, manifest).await?;
	write_installed_manifest(&paths.extracted, manifest).await
}

fn flatten_single_game_root(destination: &Path) -> Result<(), String>
{
	if destination.join("meta.json").is_file() { return Ok(()); }

	let entries = std::fs::read_dir(destination)
		.map_err(|error| format!("ZIPの展開先を確認できません: {error}"))?
		.collect::<Result<Vec<_>, _>>()
		.map_err(|error| format!("ZIPの展開先を確認できません: {error}"))?;
	let [only] = entries.as_slice() else { return Ok(()); };
	let wrapper = only.path();
	if !wrapper.is_dir() || !wrapper.join("meta.json").is_file() { return Ok(()); }

	let children = std::fs::read_dir(&wrapper)
		.map_err(|error| format!("ZIP内のゲームフォルダーを確認できません: {error}"))?
		.collect::<Result<Vec<_>, _>>()
		.map_err(|error| format!("ZIP内のゲームフォルダーを確認できません: {error}"))?;
	for child in children
	{
		let target = destination.join(child.file_name());
		std::fs::rename(child.path(), target)
			.map_err(|error| format!("ZIP内のゲームフォルダーを直下へ移動できません: {error}"))?;
	}
	std::fs::remove_dir(&wrapper)
		.map_err(|error| format!("ZIP内の空フォルダーを削除できません: {error}"))?;
	Ok(())
}

async fn download_full_archive(
	client: &mut crate::env::gamelauncher::game_service_client::GameServiceClient<tonic::transport::Channel>,
	request: DownloadRequest,
	paths: &UpdatePaths,
	progress: &DownloadProgress,
) -> Result<(), String>
{
	let response = client.download_game(Request::new(request)).await.map_err(|e| e.to_string())?;
	let mut stream = response.into_inner();
	let archive_path = paths.archive.clone();
	let (disk_sender, mut disk_receiver) = tokio::sync::mpsc::channel::<Vec<u8>>(FULL_ARCHIVE_WRITE_QUEUE_DEPTH);
	let writer_task = tokio::spawn(async move {
		let file = tokio::fs::OpenOptions::new()
			.create_new(true)
			.write(true)
			.open(archive_path)
			.await
			.map_err(|error| error.to_string())?;
		// Four 2 MiB network chunks are joined into one 8 MiB buffered disk write.
		let mut writer = tokio::io::BufWriter::with_capacity(DISK_WRITE_BUFFER_SIZE, file);
		while let Some(data) = disk_receiver.recv().await
		{
			writer.write_all(&data).await.map_err(|error| error.to_string())?;
		}
		writer.flush().await.map_err(|error| error.to_string())
	});

	let receive_result = async {
		let mut expected_index = 0;
		let mut received_data = false;
		while let Some(chunk) = stream.message().await.map_err(|error| error.to_string())?
		{
			if chunk.index != expected_index
			{
				return Err("ダウンロードデータの順序が不正です".to_string());
			}
			expected_index += 1;
			received_data = true;
			progress.add_received(chunk.data.len() as u64);
			disk_sender.send(chunk.data).await.map_err(|_| "ダウンロードデータを書き込めません".to_string())?;
		}
		if received_data { Ok(()) } else { Err("ダウンロードデータが空です".to_string()) }
	}.await;
	drop(disk_sender);
	let write_result = writer_task.await.map_err(|error| error.to_string())?;
	write_result?;
	receive_result
}

async fn install_differential_update(
	app_handle: &AppHandle,
	client: &mut crate::env::gamelauncher::game_service_client::GameServiceClient<tonic::transport::Channel>,
	game_id: &str,
	version: &str,
	game_path: &Path,
	paths: &UpdatePaths,
	manifest: &GameManifest,
) -> Result<(), String>
{
	let installed = load_or_scan_manifest(game_path).await?;
	let changed_files = changed_files(game_path, manifest, &installed).await?;
	if should_use_full_download(manifest, &changed_files)
	{
		return install_full_download(app_handle, game_id, client, DownloadRequest {
			game_id: game_id.to_string(),
			version: version.to_string(),
		}, paths, manifest).await;
	}
	let changed_set = changed_files.iter().map(|file| file.path.clone()).collect::<HashSet<_>>();
	let progress = DownloadProgress::new(
		app_handle,
		game_id,
		changed_files.iter().map(|file| file.size).sum(),
	);
	progress.stage("downloading");

	let source = game_path.to_path_buf();
	let staging = paths.extracted.clone();
	let manifest_files = manifest.files.clone();
	tokio::task::spawn_blocking(move || prepare_staging(&source, &staging, &manifest_files, &changed_set))
		.await
		.map_err(|error| error.to_string())??;

	if !changed_files.is_empty()
	{
		let groups = parallel_file_groups(&changed_files, PARALLEL_FILE_STREAMS);
		let mut tasks = Vec::with_capacity(groups.len());
		for files in groups
		{
			let mut stream_client = client.clone();
			let request_game_id = game_id.to_string();
			let request_version = version.to_string();
			let staging = paths.extracted.clone();
			let stream_progress = progress.clone();
			tasks.push(tokio::spawn(async move {
				let response = stream_client.download_game_files(Request::new(GameFilesRequest {
					game_id: request_game_id,
					version: request_version,
					paths: files.iter().map(|file| file.path.clone()).collect(),
				})).await.map_err(|error| error.message().to_string())?;
				write_file_stream(response.into_inner(), &staging, &files, &stream_progress).await
			}));
		}
		for task in tasks
		{
			task.await.map_err(|error| error.to_string())??;
		}
		verify_files(&paths.extracted, &changed_files).await?;
	}

	write_installed_manifest(&paths.extracted, manifest).await
}

fn should_use_full_download(manifest: &GameManifest, changed_files: &[GameFile]) -> bool
{
	let changed_bytes = changed_files.iter().map(|file| file.size).sum::<u64>();
	manifest.archive_size > 0 && changed_bytes >= manifest.archive_size
}

async fn load_or_scan_manifest(game_path: &Path) -> Result<InstalledManifest, String>
{
	let manifest_path = game_path.join(MANIFEST_FILE);
	if let Ok(content) = tokio::fs::read_to_string(&manifest_path).await
	{
		if let Ok(manifest) = serde_json::from_str(&content)
		{
			return Ok(manifest);
		}
	}

	let root = game_path.to_path_buf();
	tokio::task::spawn_blocking(move || scan_installed_files(&root))
		.await
		.map_err(|error| error.to_string())?
}

fn scan_installed_files(root: &Path) -> Result<InstalledManifest, String>
{
	fn visit(root: &Path, directory: &Path, files: &mut Vec<InstalledFile>) -> Result<(), String>
	{
		for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())?
		{
			let entry = entry.map_err(|error| error.to_string())?;
			let path = entry.path();
			if path.is_dir()
			{
				visit(root, &path, files)?;
			}
			else if path.file_name().and_then(|name| name.to_str()) != Some(MANIFEST_FILE)
			{
				let relative = relative_path(root, &path)?;
				let metadata = entry.metadata().map_err(|error| error.to_string())?;
				files.push(InstalledFile {
					path: relative,
					size: metadata.len(),
					sha256: hash_file(&path)?,
				});
			}
		}
		Ok(())
	}

	let mut files = Vec::new();
	visit(root, root, &mut files)?;
	files.sort_by(|left, right| left.path.cmp(&right.path));
	Ok(InstalledManifest { version: String::new(), files })
}

async fn changed_files(
	game_path: &Path,
	server: &GameManifest,
	installed: &InstalledManifest,
) -> Result<Vec<GameFile>, String>
{
	let installed = installed.files.iter()
		.map(|file| (file.path.as_str(), file))
		.collect::<HashMap<_, _>>();
	let mut changed = Vec::new();
	for server_file in &server.files
	{
		let local_path = crate::env::safe_game_relative_path(game_path, &server_file.path)?;
		let matches = installed.get(server_file.path.as_str()).is_some_and(|local| {
			local.sha256 == server_file.sha256
				&& local.size == server_file.size
				&& local_path.is_file()
		});
		if !matches { changed.push(server_file.clone()); }
	}
	Ok(changed)
}

fn prepare_staging(
	source: &Path,
	staging: &Path,
	manifest: &[GameFile],
	changed: &HashSet<String>,
) -> Result<(), String>
{
	for file in manifest
	{
		let destination = crate::env::safe_game_relative_path(staging, &file.path)?;
		if let Some(parent) = destination.parent()
		{
			std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
		}
		if changed.contains(&file.path) { continue; }

		let source_file = crate::env::safe_game_relative_path(source, &file.path)?;
		if std::fs::hard_link(&source_file, &destination).is_err()
		{
			std::fs::copy(&source_file, &destination).map_err(|error| {
				format!("既存ファイルをステージングできません ({}): {error}", file.path)
			})?;
		}
	}
	Ok(())
}

async fn write_file_stream(
	mut stream: tonic::Streaming<crate::env::gamelauncher::GameFileData>,
	staging: &Path,
	files: &[GameFile],
	progress: &DownloadProgress,
) -> Result<(), String>
{
	let staging = staging.to_path_buf();
	let files = files.to_vec();
	let (disk_sender, mut disk_receiver) = tokio::sync::mpsc::channel::<crate::env::gamelauncher::GameFileData>(FILE_WRITE_QUEUE_DEPTH);
	let writer_task = tokio::spawn(async move {
		let mut writer = FileStreamWriter::new(&staging, &files);
		while let Some(chunk) = disk_receiver.recv().await
		{
			writer.accept(chunk).await?;
		}
		writer.finish()
	});

	let receive_result = async {
		while let Some(chunk) = stream.message().await.map_err(|error| error.to_string())?
		{
			if !chunk.complete { progress.add_received(chunk.data.len() as u64); }
			disk_sender.send(chunk).await.map_err(|_| "差分データを書き込めません".to_string())?;
		}
		Ok(())
	}.await;
	drop(disk_sender);
	let write_result = writer_task.await.map_err(|error| error.to_string())?;
	write_result?;
	receive_result
}

fn parallel_file_groups(files: &[GameFile], maximum_groups: usize) -> Vec<Vec<GameFile>>
{
	let group_count = maximum_groups.max(1).min(files.len());
	if group_count == 0 { return Vec::new(); }
	let mut sorted = files.to_vec();
	sorted.sort_by(|left, right| right.size.cmp(&left.size));
	let mut groups = vec![Vec::new(); group_count];
	let mut sizes = vec![0u64; group_count];
	for file in sorted
	{
		let index = sizes.iter().enumerate().min_by_key(|(_, size)| **size).map(|(index, _)| index).unwrap_or(0);
		sizes[index] = sizes[index].saturating_add(file.size);
		groups[index].push(file);
	}
	groups
}

struct FileStreamWriter
{
	staging: PathBuf,
	expected_sizes: HashMap<String, u64>,
	current: Option<(String, tokio::io::BufWriter<tokio::fs::File>, u64)>,
}

impl FileStreamWriter
{
	fn new(staging: &Path, files: &[GameFile]) -> Self
	{
		Self {
			staging: staging.to_path_buf(),
			expected_sizes: files.iter().map(|file| (file.path.clone(), file.size)).collect(),
			current: None,
		}
	}

	async fn accept(&mut self, chunk: crate::env::gamelauncher::GameFileData) -> Result<(), String>
	{
		let expected_size = *self.expected_sizes.get(&chunk.path)
			.ok_or_else(|| format!("要求していないファイルを受信しました: {}", chunk.path))?;
		if chunk.complete
		{
			self.ensure_file_started(&chunk.path, chunk.offset).await?;
			let (path, mut file, received) = self.current.take().unwrap();
			if path != chunk.path || received != chunk.offset || received != expected_size
			{
				return Err(format!("ファイルサイズが一致しません: {}", chunk.path));
			}
			file.flush().await.map_err(|error| error.to_string())?;
			return Ok(());
		}

		self.ensure_file_started(&chunk.path, chunk.offset).await?;
		let (path, file, received) = self.current.as_mut().unwrap();
		if path != &chunk.path || *received != chunk.offset
		{
			return Err(format!("ファイルチャンクの順序が不正です: {}", chunk.path));
		}
		file.write_all(&chunk.data).await.map_err(|error| error.to_string())?;
		*received += chunk.data.len() as u64;
		Ok(())
	}

	async fn ensure_file_started(&mut self, path: &str, offset: u64) -> Result<(), String>
	{
		if self.current.is_some() { return Ok(()); }
		if offset != 0 { return Err(format!("ファイルの開始位置が不正です: {path}")); }
		let destination = crate::env::safe_game_relative_path(&self.staging, path)?;
		let file = tokio::fs::File::create(destination).await.map_err(|error| error.to_string())?;
		let writer = tokio::io::BufWriter::with_capacity(DISK_WRITE_BUFFER_SIZE, file);
		self.current = Some((path.to_string(), writer, 0));
		Ok(())
	}

	fn finish(self) -> Result<(), String>
	{
		if self.current.is_some() { Err("ファイル転送が途中で終了しました".to_string()) } else { Ok(()) }
	}
}

async fn verify_manifest(root: &Path, manifest: &GameManifest) -> Result<(), String>
{
	verify_files(root, &manifest.files).await
}

async fn verify_files(root: &Path, files: &[GameFile]) -> Result<(), String>
{
	let root = root.to_path_buf();
	let files = files.to_vec();
	tokio::task::spawn_blocking(move || {
		for file in files
		{
			let path = crate::env::safe_game_relative_path(&root, &file.path)?;
			let metadata = std::fs::metadata(&path)
				.map_err(|error| format!("受信ファイルを確認できません ({}): {error}", file.path))?;
			if metadata.len() != file.size || hash_file(&path)? != file.sha256
			{
				return Err(format!("受信ファイルの検証に失敗しました: {}", file.path));
			}
		}
		Ok(())
	}).await.map_err(|error| error.to_string())?
}

async fn write_installed_manifest(root: &Path, manifest: &GameManifest) -> Result<(), String>
{
	let installed = InstalledManifest {
		version: manifest.version.clone(),
		files: manifest.files.iter().map(|file| InstalledFile {
			path: file.path.clone(),
			size: file.size,
			sha256: file.sha256.clone(),
		}).collect(),
	};
	let json = serde_json::to_string_pretty(&installed).map_err(|error| error.to_string())?;
	tokio::fs::write(root.join(MANIFEST_FILE), json).await.map_err(|error| error.to_string())
}

fn hash_file(path: &Path) -> Result<String, String>
{
	let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
	let mut hasher = Sha256::new();
	let mut buffer = vec![0u8; 1024 * 1024];
	loop
	{
		let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
		if read == 0 { break; }
		hasher.update(&buffer[..read]);
	}
	Ok(format!("{:x}", hasher.finalize()))
}

fn relative_path(root: &Path, path: &Path) -> Result<String, String>
{
	let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
	Ok(relative.components().filter_map(|component| match component {
		Component::Normal(value) => Some(value.to_string_lossy()),
		_ => None,
	}).collect::<Vec<_>>().join("/"))
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

	fn test_root(name: &str) -> PathBuf
	{
		std::env::temp_dir().join(format!(
			"gamelauncher-{name}-{}-{}",
			std::process::id(),
			std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
		))
	}

	#[tokio::test]
	async fn storage_locks_only_serialize_the_same_game()
	{
		let first = game_storage_lock("parallel-lock-a").await;
		let same = game_storage_lock("parallel-lock-a").await;
		let other = game_storage_lock("parallel-lock-b").await;

		assert!(std::sync::Arc::ptr_eq(&first, &same));
		assert!(!std::sync::Arc::ptr_eq(&first, &other));
	}

	#[tokio::test]
	async fn replaces_existing_game_and_removes_backup()
	{
		let root = test_root("install");
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

	#[test]
	fn flattens_single_wrapping_directory()
	{
		let root = test_root("flatten");
		let wrapper = root.join("wrapped");
		std::fs::create_dir_all(&wrapper).unwrap();
		std::fs::write(wrapper.join("meta.json"), "{}").unwrap();
		std::fs::write(wrapper.join("game.exe"), "game").unwrap();

		flatten_single_game_root(&root).unwrap();

		assert!(root.join("meta.json").is_file());
		assert!(root.join("game.exe").is_file());
		assert!(!wrapper.exists());
		let _ = std::fs::remove_dir_all(root);
	}

	#[test]
	fn balances_changed_files_across_parallel_http2_streams()
	{
		let files = [9, 8, 7, 6, 5, 4].into_iter().enumerate().map(|(index, size)| GameFile {
			path: format!("file-{index}"),
			size,
			sha256: String::new(),
		}).collect::<Vec<_>>();
		let groups = parallel_file_groups(&files, 4);

		assert_eq!(groups.len(), 4);
		assert_eq!(groups.iter().map(Vec::len).sum::<usize>(), files.len());
		assert!(groups.iter().all(|group| !group.is_empty()));
	}

	#[tokio::test]
	async fn selects_only_changed_files_and_omits_deleted_files()
	{
		let root = test_root("delta");
		let game = root.join("game");
		let staging = root.join("staging");
		std::fs::create_dir_all(&game).unwrap();
		std::fs::create_dir_all(&staging).unwrap();
		std::fs::write(game.join("changed.txt"), "old").unwrap();
		std::fs::write(game.join("same.txt"), "same").unwrap();
		std::fs::write(game.join("deleted.txt"), "delete me").unwrap();

		let installed = InstalledManifest {
			version: "1.0.0".into(),
			files: vec![
				InstalledFile { path: "changed.txt".into(), size: 3, sha256: hash_file(&game.join("changed.txt")).unwrap() },
				InstalledFile { path: "same.txt".into(), size: 4, sha256: hash_file(&game.join("same.txt")).unwrap() },
				InstalledFile { path: "deleted.txt".into(), size: 9, sha256: hash_file(&game.join("deleted.txt")).unwrap() },
			],
		};
		let manifest = GameManifest {
			game_id: "game".into(),
			version: "1.1.0".into(),
			archive_size: 100,
			files: vec![
				GameFile { path: "changed.txt".into(), size: 3, sha256: format!("{:x}", Sha256::digest(b"new")) },
				GameFile { path: "same.txt".into(), size: 4, sha256: hash_file(&game.join("same.txt")).unwrap() },
			],
		};

		let changed = changed_files(&game, &manifest, &installed).await.unwrap();
		assert_eq!(changed.iter().map(|file| file.path.as_str()).collect::<Vec<_>>(), ["changed.txt"]);
		let changed_set = changed.iter().map(|file| file.path.clone()).collect();
		prepare_staging(&game, &staging, &manifest.files, &changed_set).unwrap();
		assert!(staging.join("same.txt").is_file());
		assert!(!staging.join("changed.txt").exists());
		assert!(!staging.join("deleted.txt").exists());
		let _ = std::fs::remove_dir_all(root);
	}

	#[tokio::test]
	async fn writes_and_validates_differential_file_chunks()
	{
		let root = test_root("chunks");
		std::fs::create_dir_all(&root).unwrap();
		let files = vec![GameFile {
			path: "data/file.bin".into(),
			size: 6,
			sha256: format!("{:x}", Sha256::digest(b"abcdef")),
		}];
		std::fs::create_dir_all(root.join("data")).unwrap();
		let mut writer = FileStreamWriter::new(&root, &files);
		writer.accept(crate::env::gamelauncher::GameFileData {
			path: "data/file.bin".into(), data: b"abc".to_vec(), offset: 0, complete: false,
		}).await.unwrap();
		writer.accept(crate::env::gamelauncher::GameFileData {
			path: "data/file.bin".into(), data: b"def".to_vec(), offset: 3, complete: false,
		}).await.unwrap();
		writer.accept(crate::env::gamelauncher::GameFileData {
			path: "data/file.bin".into(), data: Vec::new(), offset: 6, complete: true,
		}).await.unwrap();
		writer.finish().unwrap();
		verify_files(&root, &files).await.unwrap();
		assert_eq!(std::fs::read(root.join("data/file.bin")).unwrap(), b"abcdef");
		let _ = std::fs::remove_dir_all(root);
	}

	#[test]
	fn uses_full_archive_only_when_it_saves_bandwidth()
	{
		let mut manifest = GameManifest {
			game_id: "game".into(),
			version: "1.1.0".into(),
			archive_size: 100,
			files: Vec::new(),
		};
		let changed = vec![GameFile { path: "large.bin".into(), size: 99, sha256: String::new() }];
		assert!(!should_use_full_download(&manifest, &changed));

		manifest.archive_size = 99;
		assert!(should_use_full_download(&manifest, &changed));
		manifest.archive_size = 0;
		assert!(!should_use_full_download(&manifest, &changed));
	}
}
