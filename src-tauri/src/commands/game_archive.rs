use std::collections::HashSet;
use std::path::{Component, Path};

pub fn extract_archive(zip_path: &Path, destination: &Path) -> Result<(), String>
{
	use std::fs::File;
	use zip::ZipArchive;

	let zip_file = File::open(zip_path).map_err(|error| error.to_string())?;
	let mut archive = ZipArchive::new(zip_file).map_err(|error| error.to_string())?;
	let mut files = Vec::new();
	let mut seen = HashSet::new();
	for index in 0..archive.len()
	{
		let file = archive.by_index(index).map_err(|error| error.to_string())?;
		let decoded_name = decode_filename(file.name_raw());
		let relative = Path::new(&decoded_name);
		if decoded_name.trim().is_empty()
			|| decoded_name.contains(':')
			|| relative.components().any(|part| !matches!(part, Component::Normal(_)))
		{
			return Err(format!("ZIP内に不正なパスがあります: {decoded_name}"));
		}
		if !seen.insert(relative.to_path_buf())
		{
			return Err(format!("ZIP内のパスが重複しています: {decoded_name}"));
		}
		if file.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000)
		{
			return Err(format!("ZIP内のシンボリックリンクは使用できません: {decoded_name}"));
		}

		let output_path = destination.join(relative);
		if file.is_dir() || decoded_name.ends_with('/') || decoded_name.ends_with('\\')
		{
			std::fs::create_dir_all(&output_path).map_err(|error| error.to_string())?;
		}
		else
		{
			if let Some(parent) = output_path.parent()
			{
				std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
			}
			files.push((index, output_path));
		}
	}
	drop(archive);

	parallel_extract_files(zip_path, files)
}

fn parallel_extract_files(zip_path: &Path, files: Vec<(usize, std::path::PathBuf)>) -> Result<(), String>
{
	use std::fs::File;
	use std::sync::atomic::{AtomicUsize, Ordering};
	use std::sync::Arc;
	use zip::ZipArchive;

	if files.is_empty() { return Ok(()); }
	let files = Arc::new(files);
	let next = AtomicUsize::new(0);
	let worker_count = std::thread::available_parallelism().map(usize::from).unwrap_or(1)
		.min(4)
		.min(files.len());
	std::thread::scope(|scope| -> Result<(), String> {
		let mut workers = Vec::with_capacity(worker_count);
		for _ in 0..worker_count
		{
			let files = Arc::clone(&files);
			let next = &next;
			workers.push(scope.spawn(move || -> Result<(), String> {
				let zip_file = File::open(zip_path).map_err(|error| error.to_string())?;
				let mut archive = ZipArchive::new(zip_file).map_err(|error| error.to_string())?;
				loop
				{
					let task = next.fetch_add(1, Ordering::Relaxed);
					let Some((entry_index, output_path)) = files.get(task) else { break; };
					let mut input = archive.by_index(*entry_index).map_err(|error| error.to_string())?;
					let mut output = File::create(output_path).map_err(|error| error.to_string())?;
					std::io::copy(&mut input, &mut output).map_err(|error| error.to_string())?;
				}
				Ok(())
			}));
		}
		for worker in workers
		{
			worker.join().map_err(|_| "ZIP展開スレッドが異常終了しました".to_string())??;
		}
		Ok(())
	})
}

fn decode_filename(raw: &[u8]) -> String
{
	if let Ok(value) = std::str::from_utf8(raw)
	{
		value.to_string()
	}
	else
	{
		let (value, _, _) = encoding_rs::SHIFT_JIS.decode(raw);
		value.into_owned()
	}
}

#[cfg(test)]
mod tests
{
	use super::*;
	use std::io::Write;
	use zip::write::SimpleFileOptions;

	#[test]
	fn extracts_zip_entries_in_parallel()
	{
		let root = std::env::temp_dir().join(format!("gamelauncher-extract-{}", std::process::id()));
		let _ = std::fs::remove_dir_all(&root);
		let archive_path = root.join("game.zip");
		let destination = root.join("output");
		std::fs::create_dir_all(&root).unwrap();
		let output = std::fs::File::create(&archive_path).unwrap();
		let mut zip = zip::ZipWriter::new(output);
		for index in 0..8
		{
			zip.start_file(format!("folder/{index}.txt"), SimpleFileOptions::default()).unwrap();
			zip.write_all(format!("file-{index}").as_bytes()).unwrap();
		}
		zip.finish().unwrap();

		extract_archive(&archive_path, &destination).unwrap();
		for index in 0..8
		{
			assert_eq!(std::fs::read_to_string(destination.join(format!("folder/{index}.txt"))).unwrap(), format!("file-{index}"));
		}
		let _ = std::fs::remove_dir_all(root);
	}
}
