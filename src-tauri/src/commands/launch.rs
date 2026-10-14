use crate::env;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, State};
#[cfg(not(target_os = "windows"))]
use tauri::{PhysicalPosition, PhysicalSize};

const CLOSE_BUTTON_SIZE: i32 = 88;
const CLOSE_BUTTON_MARGIN: i32 = 24;

struct OverlayGeometry
{
	x: i32,
	y: i32,
	width: u32,
	height: u32,
	close_left: i32,
	close_top: i32,
	close_right: i32,
	close_bottom: i32,
}

#[derive(Clone, Default)]
pub struct GameProcessState
{
	child: Arc<Mutex<Option<Child>>>,
	active_game: Arc<Mutex<Option<ActiveGame>>>,
	shutting_down: Arc<AtomicBool>,
	confirming_close: Arc<AtomicBool>,
	game_suspended: Arc<AtomicBool>,
	terminating: Arc<AtomicBool>,
	control: Arc<tokio::sync::Mutex<()>>,
}

struct ActiveGame
{
	process_id: u32,
	game_id: String,
	session_token: String,
}

impl GameProcessState
{
	pub fn game_id_for_session(&self, session_token: &str) -> Result<Option<String>, String>
	{
		self.active_game
			.lock()
			.map(|active| active.as_ref().and_then(|game| {
				(game.session_token == session_token).then(|| game.game_id.clone())
			}))
			.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())
	}
}

#[tauri::command]
pub fn get_running_game_id(process_state: State<'_, GameProcessState>) -> Result<Option<String>, String>
{
	let mut child = process_state.child.lock()
		.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())?;
	let Some(process) = child.as_mut() else {
		return if process_state.terminating.load(Ordering::SeqCst) {
			process_state.active_game.lock().map(|active| active.as_ref().map(|game| game.game_id.clone()))
				.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())
		} else { Ok(None) };
	};
	if process.try_wait().map_err(|error| format!("ゲームプロセスの確認に失敗しました: {error}"))?.is_some() {
		// 終了処理とオーバーレイの片付けは既存の監視タスクに任せる。
		return Ok(None);
	}
	let active = process_state.active_game.lock()
		.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())?;
	Ok(active.as_ref().filter(|game| game.process_id == process.id()).map(|game| game.game_id.clone()))
}

#[tauri::command]
pub fn is_game_running(process_state: State<'_, GameProcessState>) -> Result<bool, String>
{
	let mut child = process_state.child.lock()
		.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())?;
	let Some(process) = child.as_mut() else { return Ok(false); };
	match process.try_wait()
	{
		Ok(None) => Ok(true),
		// Only the exit monitor owns cleanup; polling must not leave an orphan overlay.
		Ok(Some(_)) => Ok(false),
		Err(error) => Err(format!("ゲームプロセスの確認に失敗しました: {error}")),
	}
}

#[tauri::command]
pub async fn launch(
	app_handle: AppHandle,
	process_state: State<'_, GameProcessState>,
	game_id: String,
) -> Result<(), String>
{
	let state = process_state.inner().clone();
	let _control = state.control.lock().await;
	let spawn_state = state.clone();
	let process_id = tokio::task::spawn_blocking(move || start_game_process(&spawn_state, game_id)).await
		.map_err(|error| format!("ゲーム起動処理に失敗しました: {error}"))??;
	if let Err(error) = show_overlay(&app_handle) {
		let _ = run_process_operation(state.clone(), terminate_current_game).await;
		hide_overlay(&app_handle);
		return Err(error);
	}
	monitor_game_exit(app_handle, state.clone(), process_id);
	Ok(())
}

fn start_game_process(process_state: &GameProcessState, game_id: String) -> Result<u32, String> {
	let games_path = env::get_games_path()?;
	let clean_id = env::normalize_game_id(&game_id)?;
	let game_dir = games_path.join(&clean_id);

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
		env::safe_game_relative_path(&game_dir, &exe)?
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

	let mut guard = process_state.child.lock().map_err(|_| "ゲームプロセスの状態を取得できません".to_string())?;
	if process_state.terminating.load(Ordering::SeqCst) || process_state.shutting_down.load(Ordering::SeqCst) {
		return Err("ゲームの終了処理中です。終了してから起動してください".to_string());
	}
	if let Some(child) = guard.as_mut() {
		match child.try_wait() {
			Ok(Some(_)) => *guard = None,
			Ok(None) => return Err("別のゲームが既に起動しています".to_string()),
			Err(error) => return Err(format!("ゲームプロセスの確認に失敗しました: {error}")),
		}
	}

	let mut command = Command::new(&exe_path);
	command.current_dir(&game_dir);
	// ゲームIDは渡さず、起動ごとに異なる資格情報だけを子プロセスへ渡す。
	// ローカルAPIはこの資格情報から起動中ゲームを特定するため、ゲーム側から
	// 別ゲームのIDを指定してランキングを変更することはできない。
	let session_token = uuid::Uuid::new_v4().simple().to_string();
	command.env("GAMELAUNCHER_SESSION_TOKEN", &session_token);
	if is_unity_game(&exe_path, &game_dir) {
		// UnityのF11切り替えを排他的フルスクリーンではなく、
		// 外部オーバーレイを表示できるボーダーレス方式に固定する。
		command.args(["-window-mode", "borderless"]);
	}
	let child = command
		.spawn()
		.map_err(|e| format!("起動に失敗しました ({}): {}", exe_path.display(), e))?;
	let process_id = child.id();
	tracing::info!(target: "gamelauncher::game", event = "game_started", game_id = %clean_id, process_id);
	*guard = Some(child);
	*process_state.active_game.lock()
		.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())? = Some(ActiveGame {
			process_id,
			game_id: clean_id,
			session_token,
		});
	process_state.confirming_close.store(false, Ordering::SeqCst);
	process_state.game_suspended.store(false, Ordering::SeqCst);
	drop(guard);
	Ok(process_id)
}

#[tauri::command]
pub async fn request_close_game(
	app_handle: AppHandle,
	process_state: State<'_, GameProcessState>,
) -> Result<(), String>
{
	let state = process_state.inner().clone();
	let _control = state.control.lock().await;
	if state.shutting_down.load(Ordering::SeqCst) { return Err("Launcherの終了処理中です".into()); }
	if process_state.confirming_close.swap(true, Ordering::SeqCst) {
		return Ok(());
	}
	if !matches!(current_process_id(&state), Ok(Some(_))) {
		process_state.confirming_close.store(false, Ordering::SeqCst);
		return Err("起動中のゲームが見つかりません".to_string());
	}
	if let Err(error) = run_process_operation(state.clone(), suspend_current_game).await {
		tracing::warn!(target: "gamelauncher::game", event = "suspend_failed", message = %error);
	}
	if let Err(error) = show_close_confirmation(&app_handle) {
		process_state.confirming_close.store(false, Ordering::SeqCst);
		let _ = run_process_operation(state.clone(), resume_current_game).await;
		let _ = show_normal_overlay(&app_handle);
		return Err(format!("ゲーム終了確認を表示できません: {error}"));
	}
	Ok(())
}

#[tauri::command]
pub async fn close_game(
	app_handle: AppHandle,
	process_state: State<'_, GameProcessState>,
) -> Result<(), String>
{
	let state = process_state.inner().clone();
	let _control = state.control.lock().await;
	let result = run_process_operation(state.clone(), terminate_current_game).await;
	if result.is_ok() {
		process_state.confirming_close.store(false, Ordering::SeqCst);
		hide_overlay(&app_handle);
	}
	result
}

#[tauri::command]
pub async fn cancel_close_game(
	app_handle: AppHandle,
	process_state: State<'_, GameProcessState>,
) -> Result<(), String>
{
	let state = process_state.inner().clone();
	let _control = state.control.lock().await;
	if state.shutting_down.load(Ordering::SeqCst) { return Err("Launcherの終了処理中です".into()); }
	run_process_operation(state.clone(), resume_current_game).await?;
	process_state.confirming_close.store(false, Ordering::SeqCst);
	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	overlay.eval("document.body.classList.add('is-positioning'); window.hideCloseConfirmation?.()")
		.map_err(|error| error.to_string())?;
	show_normal_overlay(&app_handle)?;
	// Focus is best-effort. A frozen game must not keep the control lock or × disabled.
	drop(_control);
	match tokio::time::timeout(std::time::Duration::from_millis(750), run_process_operation(state.clone(), focus_current_game)).await {
		Ok(Ok(())) => {},
		Ok(Err(error)) => tracing::warn!(target: "gamelauncher::game", event = "game_focus_failed", message = %error),
		Err(_) => tracing::warn!(target: "gamelauncher::game", event = "game_focus_timeout"),
	}
	Ok(())
}

/// Game window/thread/process operations must never occupy the WebView's UI thread.
async fn run_process_operation(
	state: GameProcessState,
	operation: fn(&GameProcessState) -> Result<(), String>,
) -> Result<(), String> {
	tokio::task::spawn_blocking(move || operation(&state)).await
		.map_err(|error| format!("ゲーム操作に失敗しました: {error}"))?
}

pub fn begin_shutdown(app_handle: AppHandle)
{
	let process_state = app_handle.state::<GameProcessState>().inner().clone();
	if process_state.shutting_down.swap(true, Ordering::SeqCst)
	{
		return;
	}

	tauri::async_runtime::spawn(async move {
		tracing::info!(target: "gamelauncher::runtime", event = "shutdown_requested");
		let _control = process_state.control.lock().await;
		if let Err(error) = run_process_operation(process_state.clone(), terminate_current_game).await {
			tracing::error!(target: "gamelauncher::runtime", event = "shutdown_game_failed", message = %error);
		}
		tracing::info!(target: "gamelauncher::runtime", event = "shutdown_game_complete");
		hide_overlay(&app_handle);
		app_handle.exit(0);
	});
}

fn show_overlay(app_handle: &AppHandle) -> Result<(), String>
{
	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	overlay
		.set_always_on_top(true)
		.map_err(|e| format!("ゲーム終了オーバーレイを準備できません: {e}"))?;
	show_normal_overlay(app_handle)
}

pub fn prepare_overlay(app_handle: &AppHandle) -> Result<(), String>
{
	// アプリ起動時に透明な画面サイズへ確定し、非表示で保持する。
	force_overlay_to_front(app_handle)?;
	if let Some(overlay) = app_handle.get_webview_window("game-overlay") {
		overlay.set_ignore_cursor_events(true).map_err(|e| e.to_string())?;
	}
	hide_overlay(app_handle);
	Ok(())
}

fn overlay_geometry(app_handle: &AppHandle) -> Result<OverlayGeometry, String>
{
	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	let launcher_monitor = app_handle
		.get_webview_window("main")
		.and_then(|window| window.current_monitor().ok().flatten());
	let monitor = match launcher_monitor {
		Some(monitor) => monitor,
		None => overlay
			.primary_monitor()
			.map_err(|e| e.to_string())?
			.ok_or_else(|| "モニター情報を取得できません".to_string())?,
	};
	let position = monitor.position();
	let monitor_size = monitor.size();
	let scale = monitor.scale_factor();
	let button_size = (CLOSE_BUTTON_SIZE as f64 * scale).round() as i32;
	let margin = (CLOSE_BUTTON_MARGIN as f64 * scale).round() as i32;
	let close_right = position.x + monitor_size.width as i32 - margin;
	let close_top = position.y + margin;
	Ok(OverlayGeometry {
		x: position.x,
		y: position.y,
		width: monitor_size.width,
		height: monitor_size.height,
		close_left: close_right - button_size,
		close_top,
		close_right,
		close_bottom: close_top + button_size,
	})
}

#[cfg(target_os = "windows")]
fn hide_overlay(app_handle: &AppHandle)
{
	if let Some(overlay) = app_handle.get_webview_window("game-overlay") {
		let _ = overlay.eval("clearTimeout(window.__overlayReadyTimer); window.__overlayReadyTimer = null; window.hideCloseConfirmation?.(); document.body.classList.remove('is-positioning', 'overlay-ready'); document.getElementById('close-game')?.classList.remove('is-hovered', 'is-pressed')");
		let _ = overlay.set_ignore_cursor_events(true);
		if let Ok(hwnd) = overlay.hwnd() {
			use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
			unsafe { let _ = ShowWindow(hwnd, SW_HIDE); }
		}
	}
}

#[cfg(not(target_os = "windows"))]
fn hide_overlay(app_handle: &AppHandle)
{
	if let Some(overlay) = app_handle.get_webview_window("game-overlay") {
		let _ = overlay.eval("clearTimeout(window.__overlayReadyTimer); window.__overlayReadyTimer = null; window.hideCloseConfirmation?.(); document.body.classList.remove('is-positioning', 'overlay-ready'); document.getElementById('close-game')?.classList.remove('is-hovered', 'is-pressed')");
		let _ = overlay.set_ignore_cursor_events(true);
		let _ = overlay.hide();
	}
}

#[derive(Debug, PartialEq)]
enum ProcessPoll { Running, Exited(Option<i32>), Replaced }

/// This function performs only process-state work. All locks are released on return.
fn poll_game_process(state: &GameProcessState, process_id: u32) -> Result<ProcessPoll, String> {
	let mut guard = state.child.lock().map_err(|_| "ゲーム状態のロックに失敗しました".to_string())?;
	let Some(child) = guard.as_mut() else {
		return Ok(if state.terminating.load(Ordering::SeqCst) { ProcessPoll::Running } else { ProcessPoll::Replaced });
	};
	if child.id() != process_id { return Ok(ProcessPoll::Replaced); }
	let status = child.try_wait().map_err(|error| error.to_string())?;
	let Some(status) = status else { return Ok(ProcessPoll::Running); };
	let mut active = state.active_game.lock().map_err(|_| "ゲーム情報のロックに失敗しました".to_string())?;
	if active.as_ref().is_some_and(|game| game.process_id == process_id) { *active = None; }
	*guard = None;
	state.confirming_close.store(false, Ordering::SeqCst);
	state.game_suspended.store(false, Ordering::SeqCst);
	Ok(ProcessPoll::Exited(status.code()))
}

fn monitor_game_exit(app_handle: AppHandle, process_state: GameProcessState, process_id: u32)
{
	tauri::async_runtime::spawn(async move {
		let overlay_pending = Arc::new(AtomicBool::new(false));
		loop {
			tokio::time::sleep(std::time::Duration::from_millis(250)).await;
			if process_state.shutting_down.load(Ordering::SeqCst) { return; }
			// Never wait for a UI response while holding child/active_game locks.
			let poll = match poll_game_process(&process_state, process_id) {
				Ok(poll) => poll,
				Err(error) => {
					tracing::error!(target: "gamelauncher::game", event = "game_monitor_failed", process_id, message = %error);
					return; // Keep process ownership; do not silently orphan a live game.
				}
			};
			if poll == ProcessPoll::Replaced { return; }
			if let ProcessPoll::Exited(exit_code) = poll {
				tracing::info!(target: "gamelauncher::game", event = "game_exited", process_id, ?exit_code);
			}
			let finished = matches!(poll, ProcessPoll::Exited(_));
			if finished || !overlay_pending.swap(true, Ordering::SeqCst) {
				let app = app_handle.clone();
				let state = process_state.clone();
				let pending = overlay_pending.clone();
				let result = app_handle.run_on_main_thread(move || {
					pending.store(false, Ordering::SeqCst);
					if state.shutting_down.load(Ordering::SeqCst) { return; }
					// An old monitor must not hide/reposition a newer game's overlay.
					let Ok(current) = current_process_id(&state) else { return; };
					if finished {
						if current.is_none() && !state.terminating.load(Ordering::SeqCst) { hide_overlay(&app); }
					} else if current == Some(process_id) && !state.confirming_close.load(Ordering::SeqCst) {
						let _ = position_normal_overlay(&app);
					}
				});
				if result.is_err() { overlay_pending.store(false, Ordering::SeqCst); }
			}
			if finished { return; }
		}
	});
}

#[cfg(target_os = "windows")]
fn show_close_confirmation(app_handle: &AppHandle) -> Result<(), String>
{
	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	overlay.eval("clearTimeout(window.__overlayReadyTimer); window.__overlayReadyTimer = null; document.body.classList.add('is-positioning'); document.body.classList.remove('is-compact', 'overlay-ready')")
		.map_err(|error| error.to_string())?;
	overlay.set_ignore_cursor_events(false).map_err(|error| error.to_string())?;
	force_overlay_to_front(app_handle)?;
	show_prepared_overlay(app_handle)?;
	overlay.set_focus().map_err(|error| error.to_string())?;
	overlay.eval("requestAnimationFrame(() => requestAnimationFrame(() => window.showCloseConfirmation?.()))")
		.map_err(|error| error.to_string())
}

#[cfg(not(target_os = "windows"))]
fn show_close_confirmation(app_handle: &AppHandle) -> Result<(), String>
{
	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	overlay.eval("document.body.classList.add('is-positioning'); document.body.classList.remove('is-compact', 'overlay-ready')")
		.map_err(|error| error.to_string())?;
	force_overlay_to_front(app_handle)?;
	overlay.set_ignore_cursor_events(false).map_err(|error| error.to_string())?;
	show_prepared_overlay(app_handle)?;
	overlay.set_focus().map_err(|error| error.to_string())?;
	overlay.eval("requestAnimationFrame(() => requestAnimationFrame(() => window.showCloseConfirmation?.()))")
		.map_err(|error| error.to_string())
}

fn is_unity_game(executable: &std::path::Path, game_directory: &std::path::Path) -> bool
{
	if game_directory.join("UnityPlayer.dll").is_file() {
		return true;
	}
	let Some(stem) = executable.file_stem().and_then(|value| value.to_str()) else { return false; };
	game_directory.join(format!("{stem}_Data")).is_dir()
}

#[cfg(target_os = "windows")]
fn force_overlay_to_front(app_handle: &AppHandle) -> Result<(), String>
{
	use windows::Win32::UI::WindowsAndMessaging::{
		SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOOWNERZORDER,
	};

	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	let hwnd = overlay.hwnd().map_err(|e| e.to_string())?;
	let geometry = overlay_geometry(app_handle)?;
	unsafe {
		SetWindowPos(
			hwnd,
			Some(HWND_TOPMOST),
			geometry.x,
			geometry.y,
			geometry.width as i32,
			geometry.height as i32,
			SWP_NOACTIVATE | SWP_NOOWNERZORDER,
		)
	}
	.map_err(|e| format!("オーバーレイを最前面にできません: {e}"))
}

#[cfg(target_os = "windows")]
fn show_normal_overlay(app_handle: &AppHandle) -> Result<(), String>
{
	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	overlay.eval("clearTimeout(window.__overlayReadyTimer); window.__overlayReadyTimer = null; document.body.classList.remove('overlay-ready'); document.body.classList.add('is-positioning', 'is-compact'); window.hideCloseConfirmation?.()")
		.map_err(|error| error.to_string())?;
	position_normal_overlay(app_handle)?;
	// 通常時のネイティブウィンドウは×と同じ大きさなので、
	// 入力透過を切り替えずに×だけがクリックを受け取れる。
	overlay.set_ignore_cursor_events(false).map_err(|error| error.to_string())?;
	show_prepared_overlay(app_handle)?;
	// 初回のShowWindowでTauri側が生成時の座標を一度復元する場合があるため、
	// 表示後にも右上座標を確定してから×の描画を許可する。
	position_normal_overlay(app_handle)?;
	// Tauriが初回表示直後に生成時の中央座標を復元するため、位置維持処理が
	// 右上を再確定するまで描画を許可しない。
	overlay.eval("window.__overlayReadyTimer = setTimeout(() => { document.body.classList.remove('is-positioning'); document.body.classList.add('overlay-ready'); window.__overlayReadyTimer = null; }, 320)")
		.map_err(|error| error.to_string())?;
	Ok(())
}

#[cfg(target_os = "windows")]
fn position_normal_overlay(app_handle: &AppHandle) -> Result<(), String>
{
	use windows::Win32::UI::WindowsAndMessaging::{
		SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOOWNERZORDER,
	};

	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	let hwnd = overlay.hwnd().map_err(|error| error.to_string())?;
	let geometry = overlay_geometry(app_handle)?;
	let width = (geometry.close_right - geometry.close_left).max(1);
	let height = (geometry.close_bottom - geometry.close_top).max(1);
	unsafe {
		SetWindowPos(
			hwnd,
			Some(HWND_TOPMOST),
			geometry.close_left,
			geometry.close_top,
			width,
			height,
			SWP_NOACTIVATE | SWP_NOOWNERZORDER,
		)
	}
	.map_err(|error| format!("終了ボタンを配置できません: {error}"))
}

#[cfg(target_os = "windows")]
fn show_prepared_overlay(app_handle: &AppHandle) -> Result<(), String>
{
	use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	let hwnd = overlay.hwnd().map_err(|e| e.to_string())?;
	unsafe { let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE); }
	Ok(())
}

#[cfg(not(target_os = "windows"))]
fn force_overlay_to_front(app_handle: &AppHandle) -> Result<(), String>
{
	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	let geometry = overlay_geometry(app_handle)?;
	overlay
		.set_size(PhysicalSize::new(geometry.width, geometry.height))
		.and_then(|_| overlay.set_position(PhysicalPosition::new(geometry.x, geometry.y)))
		.and_then(|_| overlay.set_always_on_top(true))
		.map_err(|e| e.to_string())
}

#[cfg(not(target_os = "windows"))]
fn show_normal_overlay(app_handle: &AppHandle) -> Result<(), String>
{
	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	let geometry = overlay_geometry(app_handle)?;
	overlay
		.set_size(PhysicalSize::new(
			(geometry.close_right - geometry.close_left).max(1) as u32,
			(geometry.close_bottom - geometry.close_top).max(1) as u32,
		))
		.and_then(|_| overlay.set_position(PhysicalPosition::new(geometry.close_left, geometry.close_top)))
		.and_then(|_| overlay.set_always_on_top(true))
		.and_then(|_| overlay.set_ignore_cursor_events(false))
		.map_err(|e| e.to_string())?;
	show_prepared_overlay(app_handle)
}

#[cfg(not(target_os = "windows"))]
fn show_prepared_overlay(app_handle: &AppHandle) -> Result<(), String>
{
	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	overlay.show().map_err(|e| e.to_string())
}

fn current_process_id(process_state: &GameProcessState) -> Result<Option<u32>, String>
{
	process_state.child
		.lock()
		.map(|child| child.as_ref().map(Child::id))
		.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())
}

fn suspend_current_game(process_state: &GameProcessState) -> Result<(), String>
{
	if process_state.game_suspended.load(Ordering::SeqCst) { return Ok(()); }
	let Some(process_id) = current_process_id(process_state)? else { return Ok(()); };
	set_process_suspended(process_id, true)?;
	process_state.game_suspended.store(true, Ordering::SeqCst);
	Ok(())
}

fn resume_current_game(process_state: &GameProcessState) -> Result<(), String>
{
	if !process_state.game_suspended.load(Ordering::SeqCst) { return Ok(()); }
	let Some(process_id) = current_process_id(process_state)? else {
		process_state.game_suspended.store(false, Ordering::SeqCst);
		return Ok(());
	};
	set_process_suspended(process_id, false)?;
	process_state.game_suspended.store(false, Ordering::SeqCst);
	Ok(())
}

fn focus_current_game(process_state: &GameProcessState) -> Result<(), String>
{
	let Some(process_id) = current_process_id(process_state)? else { return Ok(()); };
	focus_process_window(process_id)
}

#[cfg(target_os = "windows")]
fn focus_process_window(process_id: u32) -> Result<(), String>
{
	use windows::core::BOOL;
	use windows::Win32::Foundation::{HWND, LPARAM, RECT};
	use windows::Win32::UI::WindowsAndMessaging::{
		EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
		IsHungAppWindow, SetForegroundWindow, ShowWindowAsync, SW_RESTORE,
	};

	struct WindowCandidate
	{
		process_id: u32,
		hwnd: Option<HWND>,
		area: i64,
	}

	unsafe extern "system" fn find_window(hwnd: HWND, lparam: LPARAM) -> BOOL
	{
		let candidate = unsafe { &mut *(lparam.0 as *mut WindowCandidate) };
		if !unsafe { IsWindowVisible(hwnd) }.as_bool() { return BOOL(1); }
		let mut owner_process_id = 0u32;
		unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_process_id)) };
		if owner_process_id != candidate.process_id { return BOOL(1); }
		let mut rect = RECT::default();
		if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok()
		{
			let area = i64::from((rect.right - rect.left).max(0))
				* i64::from((rect.bottom - rect.top).max(0));
			if area > candidate.area {
				candidate.area = area;
				candidate.hwnd = Some(hwnd);
			}
		}
		BOOL(1)
	}

	let mut candidate = WindowCandidate { process_id, hwnd: None, area: 0 };
	unsafe { EnumWindows(Some(find_window), LPARAM((&mut candidate as *mut WindowCandidate) as isize)) }
		.map_err(|error| format!("ゲームウィンドウを検索できません: {error}"))?;
	let hwnd = candidate.hwnd.ok_or_else(|| "ゲームウィンドウが見つかりません".to_string())?;
	if unsafe { IsHungAppWindow(hwnd) }.as_bool() { return Err("ゲームウィンドウが応答していません".into()); }
	if unsafe { IsIconic(hwnd) }.as_bool() {
		// Do not synchronously wait for the game's window procedure when it is hung.
		unsafe { let _ = ShowWindowAsync(hwnd, SW_RESTORE); }
	}
	if !unsafe { SetForegroundWindow(hwnd) }.as_bool() {
		return Err("ゲームウィンドウへフォーカスを戻せません".to_string());
	}
	Ok(())
}

#[cfg(not(target_os = "windows"))]
fn focus_process_window(_process_id: u32) -> Result<(), String>
{
	Ok(())
}

#[cfg(target_os = "windows")]
fn set_process_suspended(process_id: u32, suspended: bool) -> Result<(), String>
{
	use windows::Win32::Foundation::CloseHandle;
	use windows::Win32::System::Diagnostics::ToolHelp::{
		CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD, THREADENTRY32,
	};
	use windows::Win32::System::Threading::{
		OpenThread, ResumeThread, SuspendThread, THREAD_SUSPEND_RESUME,
	};

	let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }
		.map_err(|error| format!("ゲームのスレッド一覧を取得できません: {error}"))?;
	let mut entry = THREADENTRY32 {
		dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
		..Default::default()
	};
	let mut changed_threads = 0usize;
	let mut next = unsafe { Thread32First(snapshot, &mut entry) };
	while next.is_ok()
	{
		if entry.th32OwnerProcessID == process_id
		{
			if let Ok(thread) = unsafe { OpenThread(THREAD_SUSPEND_RESUME, false, entry.th32ThreadID) }
			{
				let previous_count = unsafe {
					if suspended { SuspendThread(thread) } else { ResumeThread(thread) }
				};
				if previous_count != u32::MAX { changed_threads += 1; }
				let _ = unsafe { CloseHandle(thread) };
			}
		}
		next = unsafe { Thread32Next(snapshot, &mut entry) };
	}
	let _ = unsafe { CloseHandle(snapshot) };
	if changed_threads == 0 {
		return Err("ゲームの実行スレッドを一時停止・再開できません".to_string());
	}
	Ok(())
}

#[cfg(not(target_os = "windows"))]
fn set_process_suspended(_process_id: u32, _suspended: bool) -> Result<(), String>
{
	// 現在サポート対象のWindows以外では確認画面のみ表示する。
	Ok(())
}

fn terminate_current_game(process_state: &GameProcessState) -> Result<(), String>
{
	if process_state.terminating.swap(true, Ordering::SeqCst) { return Err("ゲームの終了処理中です".into()); }
	struct ResetFlag<'a>(&'a AtomicBool);
	impl Drop for ResetFlag<'_> { fn drop(&mut self) { self.0.store(false, Ordering::SeqCst); } }
	let _reset = ResetFlag(&process_state.terminating);
	// Take ownership only briefly. Process waits happen with no state lock held.
	let mut child = process_state
		.child
		.lock()
		.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())?
		.take();
	if let Some(process) = child.as_mut() {
		let process_id = process.id();
		tracing::info!(target: "gamelauncher::game", event = "game_termination_started", process_id);
		if let Err(error) = terminate_process_tree(process) {
			*process_state.child.lock().map_err(|_| "ゲーム状態の復元に失敗しました".to_string())? = child;
			tracing::error!(target: "gamelauncher::game", event = "game_termination_failed", process_id, message = %error);
			return Err(error);
		}
		tracing::info!(target: "gamelauncher::game", event = "game_termination_complete", process_id);
	}
	*process_state.active_game.lock().map_err(|_| "ゲームプロセスの状態を取得できません".to_string())? = None;
	process_state.game_suspended.store(false, Ordering::SeqCst);
	Ok(())
}

pub async fn terminate_game_if_running(app_handle: &AppHandle, game_id: &str) -> Result<(), String>
{
	let process_state = app_handle.state::<GameProcessState>().inner().clone();
	let _control = process_state.control.lock().await;
	let is_target_running = process_state.active_game.lock()
		.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())?
		.as_ref()
		.is_some_and(|active| active.game_id == game_id);
	if is_target_running
	{
		run_process_operation(process_state.clone(), terminate_current_game).await?;
		hide_overlay(app_handle);
	}
	Ok(())
}

#[cfg(target_os = "windows")]
fn terminate_process_tree(child: &mut Child) -> Result<(), String>
{
	use std::os::windows::process::CommandExt;
	const CREATE_NO_WINDOW: u32 = 0x08000000;

	if child.try_wait().map_err(|e| e.to_string())?.is_some() {
		return Ok(());
	}
	let process_id = child.id().to_string();
	let task = Command::new("taskkill")
		.args(["/PID", &process_id, "/T", "/F"])
		.creation_flags(CREATE_NO_WINDOW)
		.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
		.spawn();
	if let Ok(mut task) = task {
		if !wait_for_exit(&mut task, std::time::Duration::from_secs(3))? {
			let _ = task.kill();
			tracing::warn!(target: "gamelauncher::game", event = "taskkill_timeout", process_id = child.id());
		}
	}
	if child.try_wait().map_err(|e| e.to_string())?.is_some() { return Ok(()); }
	child.kill().map_err(|e| format!("ゲームを終了できませんでした: {e}"))?;
	if wait_for_exit(child, std::time::Duration::from_secs(3))? { Ok(()) }
	else { Err("ゲームの強制終了が時間内に完了しませんでした。再試行してください".into()) }
}

fn wait_for_exit(child: &mut Child, timeout: std::time::Duration) -> Result<bool, String> {
	let deadline = std::time::Instant::now() + timeout;
	loop {
		if child.try_wait().map_err(|error| error.to_string())?.is_some() { return Ok(true); }
		if std::time::Instant::now() >= deadline { return Ok(false); }
		std::thread::sleep(std::time::Duration::from_millis(20));
	}
}

#[cfg(not(target_os = "windows"))]
fn terminate_process_tree(child: &mut Child) -> Result<(), String>
{
	if child.try_wait().map_err(|e| e.to_string())?.is_none() {
		child.kill().map_err(|e| format!("ゲームを終了できませんでした: {e}"))?;
		if !wait_for_exit(child, std::time::Duration::from_secs(3))? {
			return Err("ゲームの強制終了が時間内に完了しませんでした".into());
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests
{
	use super::*;

	// A real child process used only by these tests, never a user's running game.
	#[test]
	#[ignore]
	fn slow_process_fixture() { std::thread::sleep(std::time::Duration::from_secs(60)); }

	struct TestGame(GameProcessState, u32);
	impl TestGame {
		fn start() -> Self {
			let mut command = Command::new(std::env::current_exe().unwrap());
			command.args(["--ignored", "--exact", "commands::launch::tests::slow_process_fixture", "--test-threads=1"])
				.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
			#[cfg(target_os = "windows")]
			{
				use std::os::windows::process::CommandExt;
				command.creation_flags(0x08000000);
			}
			let child = command.spawn().unwrap();
			let process_id = child.id();
			let state = GameProcessState::default();
			*state.child.lock().unwrap() = Some(child);
			*state.active_game.lock().unwrap() = Some(ActiveGame { process_id, game_id: "fixture".into(), session_token: "fixture-token".into() });
			Self(state, process_id)
		}
	}
	impl Drop for TestGame { fn drop(&mut self) { let _ = terminate_current_game(&self.0); } }

	#[test]
	fn monitor_releases_all_state_locks_before_ui_work() {
		let game = TestGame::start();
		assert_eq!(poll_game_process(&game.0, game.1).unwrap(), ProcessPoll::Running);
		// Simulate a UI getter callback which reads state; it must not deadlock.
		assert!(game.0.child.try_lock().is_ok());
		assert!(game.0.active_game.try_lock().is_ok());
		assert_eq!(current_process_id(&game.0).unwrap(), Some(game.1));
		assert_eq!(poll_game_process(&game.0, game.1.wrapping_add(1)).unwrap(), ProcessPoll::Replaced);
	}

	#[test]
	fn process_wait_is_bounded_and_does_not_require_game_responses() {
		let game = TestGame::start();
		let mut child = game.0.child.lock().unwrap().take().unwrap();
		assert!(!wait_for_exit(&mut child, std::time::Duration::from_millis(30)).unwrap());
		*game.0.child.lock().unwrap() = Some(child);
		#[cfg(target_os = "windows")]
		set_process_suspended(game.1, true).unwrap();
		// Suspended processes remain running, but polling and session lookups still work.
		assert_eq!(poll_game_process(&game.0, game.1).unwrap(), ProcessPoll::Running);
		assert_eq!(game.0.game_id_for_session("fixture-token").unwrap(), Some("fixture".into()));
		terminate_current_game(&game.0).unwrap();
		assert_eq!(current_process_id(&game.0).unwrap(), None);
		assert_eq!(game.0.game_id_for_session("fixture-token").unwrap(), None);
		assert!(!game.0.terminating.load(Ordering::SeqCst));
	}

	#[tokio::test(flavor = "current_thread")]
	async fn blocking_game_operation_does_not_block_the_async_event_loop() {
		fn slow_operation(state: &GameProcessState) -> Result<(), String> {
			assert!(state.child.try_lock().is_ok());
			std::thread::sleep(std::time::Duration::from_millis(150));
			Ok(())
		}
		let task = tokio::spawn(run_process_operation(GameProcessState::default(), slow_operation));
		tokio::time::timeout(std::time::Duration::from_millis(100), tokio::time::sleep(std::time::Duration::from_millis(10))).await.unwrap();
		assert!(!task.is_finished());
		task.await.unwrap().unwrap();
	}

	#[test]
	fn termination_owns_the_child_without_discarding_session_or_stopping_monitoring() {
		let game = TestGame::start();
		game.0.terminating.store(true, Ordering::SeqCst);
		let child = game.0.child.lock().unwrap().take();
		assert_eq!(poll_game_process(&game.0, game.1).unwrap(), ProcessPoll::Running);
		assert_eq!(game.0.game_id_for_session("fixture-token").unwrap(), Some("fixture".into()));
		*game.0.child.lock().unwrap() = child;
		game.0.terminating.store(false, Ordering::SeqCst);
	}

	#[test]
	fn monitor_cleans_up_only_its_exited_game() {
		let game = TestGame::start();
		game.0.confirming_close.store(true, Ordering::SeqCst);
		game.0.game_suspended.store(true, Ordering::SeqCst);
		{
			let mut child = game.0.child.lock().unwrap();
			let process = child.as_mut().unwrap();
			process.kill().unwrap();
			assert!(wait_for_exit(process, std::time::Duration::from_secs(2)).unwrap());
		}
		assert!(matches!(poll_game_process(&game.0, game.1).unwrap(), ProcessPoll::Exited(_)));
		assert!(game.0.child.try_lock().unwrap().is_none());
		assert!(game.0.active_game.try_lock().unwrap().is_none());
		assert!(!game.0.confirming_close.load(Ordering::SeqCst));
		assert!(!game.0.game_suspended.load(Ordering::SeqCst));
		assert_eq!(poll_game_process(&game.0, game.1).unwrap(), ProcessPoll::Replaced);
	}

	#[test]
	fn detects_unity_player_layout()
	{
		let root = std::env::temp_dir().join(format!(
			"gamelauncher-unity-test-{}-{}",
			std::process::id(),
			std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos(),
		));
		std::fs::create_dir_all(root.join("Sample_Data")).unwrap();
		assert!(is_unity_game(&root.join("Sample.exe"), &root));
		assert!(!is_unity_game(&root.join("Other.exe"), &root));
		let _ = std::fs::remove_dir_all(root);
	}

	#[test]
	fn session_token_only_resolves_its_launched_game()
	{
		let state = GameProcessState::default();
		*state.active_game.lock().unwrap() = Some(ActiveGame {
			process_id: 42,
			game_id: "game-a".into(),
			session_token: "secret-a".into(),
		});
		assert_eq!(state.game_id_for_session("secret-a").unwrap().as_deref(), Some("game-a"));
		assert_eq!(state.game_id_for_session("secret-b").unwrap(), None);
	}
}
