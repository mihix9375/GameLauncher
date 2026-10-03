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
	let Some(process) = child.as_mut() else { return Ok(None); };
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
		Ok(Some(_)) => {
			*child = None;
			process_state.game_suspended.store(false, Ordering::SeqCst);
			*process_state.active_game.lock()
				.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())? = None;
			Ok(false)
		}
		Err(error) => Err(format!("ゲームプロセスの確認に失敗しました: {error}")),
	}
}

#[tauri::command]
pub fn launch(
	app_handle: AppHandle,
	process_state: State<'_, GameProcessState>,
	game_id: String,
) -> Result<(), String>
{
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

	if let Err(error) = show_overlay(&app_handle) {
		let _ = terminate_current_game(process_state.inner());
		return Err(error);
	}
	monitor_game_exit(app_handle.clone(), process_state.inner().clone(), process_id);
	Ok(())
}

#[tauri::command]
pub fn request_close_game(
	app_handle: AppHandle,
	process_state: State<'_, GameProcessState>,
) -> Result<(), String>
{
	if process_state.confirming_close.swap(true, Ordering::SeqCst) {
		return Ok(());
	}
	if current_process_id(process_state.inner())?.is_none() {
		process_state.confirming_close.store(false, Ordering::SeqCst);
		return Err("起動中のゲームが見つかりません".to_string());
	}
	if let Err(error) = suspend_current_game(process_state.inner()) {
		tracing::warn!(target: "gamelauncher::game", event = "suspend_failed", message = %error);
	}
	if let Err(error) = show_close_confirmation(&app_handle) {
		process_state.confirming_close.store(false, Ordering::SeqCst);
		let _ = resume_current_game(process_state.inner());
		let _ = show_normal_overlay(&app_handle);
		return Err(format!("ゲーム終了確認を表示できません: {error}"));
	}
	Ok(())
}

#[tauri::command]
pub fn close_game(
	app_handle: AppHandle,
	process_state: State<'_, GameProcessState>,
) -> Result<(), String>
{
	process_state.confirming_close.store(false, Ordering::SeqCst);
	process_state.game_suspended.store(false, Ordering::SeqCst);
	let result = terminate_current_game(process_state.inner());
	hide_overlay(&app_handle);
	result
}

#[tauri::command]
pub fn cancel_close_game(
	app_handle: AppHandle,
	process_state: State<'_, GameProcessState>,
) -> Result<(), String>
{
	resume_current_game(process_state.inner())?;
	process_state.confirming_close.store(false, Ordering::SeqCst);
	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	overlay.eval("document.body.classList.add('is-positioning'); window.hideCloseConfirmation?.()")
		.map_err(|error| error.to_string())?;
	show_normal_overlay(&app_handle)?;
	focus_current_game(process_state.inner())
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
		let state_for_termination = process_state.clone();
		let _ = tokio::task::spawn_blocking(move || terminate_current_game(&state_for_termination)).await;
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

fn monitor_game_exit(app_handle: AppHandle, process_state: GameProcessState, process_id: u32)
{
	tauri::async_runtime::spawn(async move {
		loop {
			tokio::time::sleep(std::time::Duration::from_millis(250)).await;
			if process_state.shutting_down.load(Ordering::SeqCst) { return; }
			let finished = {
				let Ok(mut guard) = process_state.child.lock() else { return; };
				let Some(child) = guard.as_mut() else { return; };
				if child.id() != process_id { return; }
				match child.try_wait() {
					Ok(Some(status)) => {
						tracing::info!(target: "gamelauncher::game", event = "game_exited", process_id, exit_code = ?status.code());
						*guard = None;
						true
					}
					Ok(None) => {
						// 確認画面の表示中に通常表示へ戻すと、入力を受け付けない
						// 透明オーバーレイだけが残る。通常時のみ最前面を維持する。
						if !process_state.confirming_close.load(Ordering::SeqCst) {
							let _ = position_normal_overlay(&app_handle);
						}
						false
					}
					Err(error) => {
						tracing::error!(target: "gamelauncher::game", event = "game_monitor_failed", process_id, message = %error);
						*guard = None;
						true
					}
				}
			};
			if finished {
				process_state.confirming_close.store(false, Ordering::SeqCst);
				process_state.game_suspended.store(false, Ordering::SeqCst);
				if let Ok(mut active) = process_state.active_game.lock()
				{
					if active.as_ref().is_some_and(|game| game.process_id == process_id) { *active = None; }
				}
				hide_overlay(&app_handle);
				return;
			}
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
		SetForegroundWindow, ShowWindow, SW_RESTORE,
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
	if unsafe { IsIconic(hwnd) }.as_bool() {
		unsafe { let _ = ShowWindow(hwnd, SW_RESTORE); }
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
	process_state.game_suspended.store(false, Ordering::SeqCst);
	let mut child = process_state
		.child
		.lock()
		.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())?
		.take();
	*process_state.active_game.lock()
		.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())? = None;
	let Some(child) = child.as_mut() else { return Ok(()); };
	terminate_process_tree(child)
}

pub fn terminate_game_if_running(app_handle: &AppHandle, game_id: &str) -> Result<(), String>
{
	let process_state = app_handle.state::<GameProcessState>();
	let is_target_running = process_state.active_game.lock()
		.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())?
		.as_ref()
		.is_some_and(|active| active.game_id == game_id);
	if is_target_running
	{
		terminate_current_game(process_state.inner())?;
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
	let status = Command::new("taskkill")
		.args(["/PID", &process_id, "/T", "/F"])
		.creation_flags(CREATE_NO_WINDOW)
		.status();
	if status.as_ref().is_ok_and(|value| value.success()) {
		let _ = child.wait();
		Ok(())
	} else {
		child.kill().map_err(|e| format!("ゲームを終了できませんでした: {e}"))?;
		let _ = child.wait();
		Ok(())
	}
}

#[cfg(not(target_os = "windows"))]
fn terminate_process_tree(child: &mut Child) -> Result<(), String>
{
	if child.try_wait().map_err(|e| e.to_string())?.is_none() {
		child.kill().map_err(|e| format!("ゲームを終了できませんでした: {e}"))?;
		let _ = child.wait();
	}
	Ok(())
}

#[cfg(test)]
mod tests
{
	use super::*;

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
