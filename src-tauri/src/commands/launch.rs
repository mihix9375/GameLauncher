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
	active_game: Arc<Mutex<Option<(u32, String)>>>,
	shutting_down: Arc<AtomicBool>,
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
	if is_unity_game(&exe_path, &game_dir) {
		// UnityのF11切り替えを排他的フルスクリーンではなく、
		// 外部オーバーレイを表示できるボーダーレス方式に固定する。
		command.args(["-window-mode", "borderless"]);
	}
	let child = command
		.spawn()
		.map_err(|e| format!("起動に失敗しました ({}): {}", exe_path.display(), e))?;
	let process_id = child.id();
	*guard = Some(child);
	*process_state.active_game.lock()
		.map_err(|_| "ゲームプロセスの状態を取得できません".to_string())? = Some((process_id, clean_id));
	drop(guard);

	if let Err(error) = show_overlay(&app_handle) {
		let _ = terminate_current_game(process_state.inner());
		return Err(error);
	}
	monitor_game_exit(app_handle.clone(), process_state.inner().clone(), process_id);
	monitor_overlay_input(app_handle, process_state.inner().clone(), process_id);
	Ok(())
}

#[tauri::command]
pub fn close_game(
	app_handle: AppHandle,
	process_state: State<'_, GameProcessState>,
) -> Result<(), String>
{
	let result = terminate_current_game(process_state.inner());
	hide_overlay(&app_handle);
	result
}

pub fn begin_shutdown(app_handle: AppHandle)
{
	let process_state = app_handle.state::<GameProcessState>().inner().clone();
	if process_state.shutting_down.swap(true, Ordering::SeqCst)
	{
		return;
	}

	tauri::async_runtime::spawn(async move {
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
	force_overlay_to_front(app_handle)?;
	show_prepared_overlay(app_handle)
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
		let _ = overlay.eval("document.getElementById('close-game')?.classList.remove('is-hovered', 'is-pressed')");
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
		let _ = overlay.eval("document.getElementById('close-game')?.classList.remove('is-hovered', 'is-pressed')");
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
					Ok(Some(_)) => {
						*guard = None;
						true
					}
					Ok(None) => {
						// 状態ロックを保持したまま表示を維持する。
						// close_gameは先に同じ状態をNoneへ変更してから非表示にするため、
						// 終了後にこの処理が×を再表示する競合は発生しない。
						let _ = force_overlay_to_front(&app_handle);
						let _ = show_prepared_overlay(&app_handle);
						false
					}
					Err(_) => {
						*guard = None;
						true
					}
				}
			};
			if finished {
				if let Ok(mut active) = process_state.active_game.lock()
				{
					if active.as_ref().is_some_and(|(id, _)| *id == process_id) { *active = None; }
				}
				hide_overlay(&app_handle);
				return;
			}
		}
	});
}

#[cfg(target_os = "windows")]
fn monitor_overlay_input(app_handle: AppHandle, process_state: GameProcessState, process_id: u32)
{
	tauri::async_runtime::spawn(async move {
		let mut was_pressed = false;
		let mut was_hovered = false;
		let mut was_visually_pressed = false;
		let Ok(geometry) = overlay_geometry(&app_handle) else { return; };
		loop {
			tokio::time::sleep(std::time::Duration::from_millis(16)).await;
			if process_state.shutting_down.load(Ordering::SeqCst) { return; }
			let running = process_state
				.child
				.lock()
				.ok()
				.and_then(|guard| guard.as_ref().map(|child| child.id() == process_id))
				.unwrap_or(false);
			let Some(overlay) = app_handle.get_webview_window("game-overlay") else { return; };
			if !running {
				return;
			}
			let pressed = left_mouse_button_pressed();
			let hovered = cursor_over_close_button(&geometry).unwrap_or(false);
			if hovered != was_hovered {
				let _ = overlay.eval(&format!(
					"document.getElementById('close-game')?.classList.toggle('is-hovered', {hovered})"
				));
				was_hovered = hovered;
			}
			let visually_pressed = hovered && pressed;
			if visually_pressed != was_visually_pressed {
				let _ = overlay.eval(&format!(
					"document.getElementById('close-game')?.classList.toggle('is-pressed', {visually_pressed})"
				));
				was_visually_pressed = visually_pressed;
			}
			if pressed && !was_pressed && hovered {
				let _ = terminate_current_game(&process_state);
				hide_overlay(&app_handle);
				return;
			}
			was_pressed = pressed;
		}
	});
}

#[cfg(not(target_os = "windows"))]
fn monitor_overlay_input(_app_handle: AppHandle, _process_state: GameProcessState, _process_id: u32) {}

#[cfg(target_os = "windows")]
fn cursor_over_close_button(geometry: &OverlayGeometry) -> Result<bool, String>
{
	use windows::Win32::Foundation::POINT;
	use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
	let mut point = POINT::default();
	unsafe { GetCursorPos(&mut point) }.map_err(|e| e.to_string())?;
	Ok(
		point.x >= geometry.close_left
			&& point.x <= geometry.close_right
			&& point.y >= geometry.close_top
			&& point.y <= geometry.close_bottom
	)
}

#[cfg(target_os = "windows")]
fn left_mouse_button_pressed() -> bool
{
	use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
	unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 }
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
fn show_prepared_overlay(app_handle: &AppHandle) -> Result<(), String>
{
	let overlay = app_handle
		.get_webview_window("game-overlay")
		.ok_or_else(|| "ゲーム終了オーバーレイが見つかりません".to_string())?;
	overlay.show().map_err(|e| e.to_string())
}

fn terminate_current_game(process_state: &GameProcessState) -> Result<(), String>
{
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
		.is_some_and(|(_, active_id)| active_id == game_id);
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
}
