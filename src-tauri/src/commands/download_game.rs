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
		let zip_file = std::fs::File::open(&zip_path_clone).map_err(|e| e.to_string())?;
		let engine = ripunzip::UnzipEngine::for_file(zip_file).map_err(|e| e.to_string())?;
		let options = ripunzip::UnzipOptions {
			output_directory: Some(game_path_clone),
			password: None,
			single_threaded: false,
			filename_filter: None,
			progress_reporter: Box::new(ripunzip::NullProgressReporter),
		};
		engine.unzip(options).map_err(|e| e.to_string())?;
		Ok(())
	})
	.await
	.map_err(|e| e.to_string())??;

	Ok(())
}
