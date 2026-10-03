use chrono::Local;
use sha2::{Digest, Sha256};
use std::{fmt, fs::OpenOptions, path::{Path, PathBuf}};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::{format::Writer, time::FormatTime};

static LOG_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct LocalTimer;
impl FormatTime for LocalTimer {
	fn format_time(&self, writer: &mut Writer<'_>) -> fmt::Result {
		write!(writer, "{}", Local::now().format("%Y-%m-%dT%H:%M:%S%.3f%:z"))
	}
}

fn session_filename(date: &str, session: &str) -> String {
	let hash = format!("{:x}", Sha256::digest(session.as_bytes()));
	format!("{date}-{}.jsonl", &hash[..16])
}

// 起動ごとに独立したJSONL。Serverと同じtimestamp/level/fields/target形式。
// guardをmainの終了まで保持し、ファイル書き込みをUIスレッドから分離する。
pub fn init(root: &Path) -> Result<(WorkerGuard, PathBuf), Box<dyn std::error::Error + Send + Sync>> {
	let directory = root.join("logs");
	std::fs::create_dir_all(&directory)?;
	let path = directory.join(session_filename(
		&Local::now().format("%Y-%m-%d").to_string(),
		&uuid::Uuid::new_v4().to_string(),
	));
	let file = OpenOptions::new().create_new(true).write(true).open(&path)?;
	let (writer, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default()
		.lossy(false).finish(file);
	tracing_subscriber::fmt().json().with_timer(LocalTimer).with_writer(writer)
		.with_ansi(false).with_current_span(false).with_span_list(false)
		.with_thread_ids(true).with_thread_names(true).with_file(true).with_line_number(true)
		.with_max_level(tracing::Level::INFO).try_init()?;
	LOG_READY.store(true, std::sync::atomic::Ordering::Release);
	let previous_hook = std::panic::take_hook();
	std::panic::set_hook(Box::new(move |panic| {
		tracing::error!(target: "gamelauncher::runtime", event = "panic", message = %panic);
		previous_hook(panic);
	}));
	Ok((guard, path))
}

#[tauri::command]
pub fn write_client_log(level: String, message: String, source: String) -> Result<(), String> {
	if !LOG_READY.load(std::sync::atomic::Ordering::Acquire) {
		return Err("ログファイルを初期化できていません".into());
	}
	emit_client_log(level, message, source)
}

fn emit_client_log(level: String, message: String, source: String) -> Result<(), String> {
	if message.len() > 65_536 || source.len() > 128 { return Err("ログが長すぎます".into()); }
	match level.as_str() {
		"ERROR" => tracing::error!(target: "gamelauncher::frontend", source, message),
		"WARN" => tracing::warn!(target: "gamelauncher::frontend", source, message),
		"INFO" => tracing::info!(target: "gamelauncher::frontend", source, message),
		_ => return Err("ログレベルが不正です".into()),
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn filenames_have_date_and_session_hash() {
		let name = session_filename("2026-10-03", "session-a");
		assert!(name.starts_with("2026-10-03-"));
		assert!(name.ends_with(".jsonl"));
		assert_ne!(name, session_filename("2026-10-03", "session-b"));
		assert_eq!(name.len(), 33);
	}
	#[test]
	fn log_input_is_bounded_and_validates_levels() {
		assert!(emit_client_log("INFO".into(), "hello\nworld".into(), "ui".into()).is_ok());
		assert!(emit_client_log("bogus".into(), "hello".into(), "ui".into()).is_err());
		assert!(emit_client_log("INFO".into(), "x".repeat(65_537), "ui".into()).is_err());
	}
	#[test]
	fn jsonl_uses_server_fields_and_escapes_newlines() {
		let path = std::env::temp_dir().join(format!("launcher-log-{}.jsonl", uuid::Uuid::new_v4()));
		let file = OpenOptions::new().create_new(true).write(true).open(&path).unwrap();
		let (writer, guard) = tracing_appender::non_blocking::NonBlockingBuilder::default().lossy(false).finish(file);
		let subscriber = tracing_subscriber::fmt().json().with_timer(LocalTimer).with_writer(writer).finish();
		tracing::subscriber::with_default(subscriber, || {
			tracing::info!(target: "gamelauncher::test", message = "first\nsecond");
		});
		drop(guard);
		let content = std::fs::read_to_string(&path).unwrap();
		assert_eq!(content.lines().count(), 1);
		let record: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
		assert!(record["timestamp"].as_str().unwrap().contains('T'));
		assert_eq!(record["level"], "INFO");
		assert_eq!(record["target"], "gamelauncher::test");
		assert_eq!(record["fields"]["message"], "first\nsecond");
		std::fs::remove_file(path).unwrap();
	}
}
