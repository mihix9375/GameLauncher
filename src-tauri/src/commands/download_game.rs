use tonic::Request;
use crate::env::gamelauncher::{
	DownloadRequest
};
use tokio::io::AsyncWriteExt;

#[tauri::command]
pub async fn download_game
(
	game_id: String, 
	version: String,
) -> Result<(), String>
{
	let games_path 	 = crate::env::get_games_path()?;
	let clean_id     = game_id.trim_end_matches(".exe");
	let game_path  	 = games_path.join(clean_id);
	let zip_path 	 = game_path.join(format!("{}.zip", clean_id));

	if let Some(parent) = std::path::Path::new(&zip_path).parent() {
		tokio::fs::create_dir_all(parent)
			.await
			.map_err(|e| e.to_string())?;
	}

	let url = crate::env::get_config().server_url;
	let mut client = crate::env::connect_and_get_client(url);

	let request = Request::new(DownloadRequest {
		game_id: clean_id.to_string(),
		version: version,
	});

	let response = client.download_game(request)
		.await
		.map_err(|e| e.to_string())?;

	let mut stream = response.into_inner();

	let mut file = tokio::fs::OpenOptions::new()
		.create(true)
		.write(true)
		.truncate(true)
		.open(&zip_path)
		.await
		.map_err(|e| e.to_string())?;

	while let Some(chunk) = stream.message().await.map_err(|e| e.to_string())?
	{
		file.write(&chunk.data)
			.await
			.map_err(|e| e.to_string())?;
	}

	file.flush().await.map_err(|e| e.to_string())?;
	std::mem::drop(file);

	let zip_path_clone = zip_path.clone();
	let game_path_clone = game_path.clone();
	tokio::task::spawn_blocking(move || -> Result<(), String> {
		use zip::ZipArchive;
		use std::fs::File;
		use std::path::Path;

		let zip_file = File::open(&zip_path_clone).map_err(|e| e.to_string())?;
		let mut archive = ZipArchive::new(zip_file).map_err(|e| e.to_string())?;

		for i in 0..archive.len() {
			let mut file = archive.by_index(i).map_err(|e| e.to_string())?;
			let raw_name = file.name_raw();
			let decoded_name = if let Ok(s) = std::str::from_utf8(raw_name) {
				s.to_string()
			} else {
				let (cow, _, _) = encoding_rs::SHIFT_JIS.decode(raw_name);
				cow.into_owned()
			};

			let mut outpath = game_path_clone.clone();
			for component in Path::new(&decoded_name).components() {
				if let std::path::Component::Normal(c) = component {
					outpath.push(c);
				}
			}

			if file.is_dir() || decoded_name.ends_with('/') || decoded_name.ends_with('\\') {
				let _ = std::fs::create_dir_all(&outpath);
			} else {
				if let Some(parent) = outpath.parent() {
					let _ = std::fs::create_dir_all(parent);
				}
				let mut outfile = File::create(&outpath).map_err(|e| e.to_string())?;
				std::io::copy(&mut file, &mut outfile).map_err(|e| e.to_string())?;
			}
		}
		Ok(())
	})
	.await
	.map_err(|e| e.to_string())??;

	Ok(())
}
