use tonic::{
	Request, transport::Channel
};
use crate::env::gamelauncher::{
	DownloadRequest
};
use crate::env::gamelauncher::game_service_client::GameServiceClient;
use tokio::io::AsyncWriteExt;

#[tauri::command]
pub async fn download_game
(
	game_id: String, 
	version: String,
	client_state: tauri::State<'_, GameServiceClient<Channel>>,
) -> Result<(), String>
{
	let games_path 	 = crate::env::get_games_path();
	let game_path  	 = games_path?.join(&game_id);
	let mut zip_name = game_id.clone();
	zip_name.push_str(".zip");
	let zip_path 	 = game_path.join(zip_name);

	if let Some(parent) = std::path::Path::new(&zip_path).parent() {
		tokio::fs::create_dir_all(parent)
			.await
			.map_err(|e| e.to_string())?;
	}

	let mut client = client_state.inner().clone();

	let request = Request::new(DownloadRequest {
		game_id: game_id,
		version: version,
	});

	let response = client.download_game(request)
		.await
		.map_err(|e| e.to_string())?;

	let mut stream = response.into_inner();

	let mut file = tokio::fs::OpenOptions::new()
		.create(true)
		.append(true)
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
	.map_err(|e| e.to_string())?;

	Ok(())
}
