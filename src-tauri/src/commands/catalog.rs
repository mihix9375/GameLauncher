use crate::server_api;

#[tauri::command]
pub async fn get_game_metadata(game_id: String) -> Result<crate::env::Meta, String>
{
	let game_id = crate::env::normalize_game_id(&game_id)?;
	let response = server_api::client()?.get(server_api::game_endpoint(&game_id, &["metadata"])?).send().await
		.map_err(|error| format!("ゲーム情報を取得できません: {error}"))?;
	if !response.status().is_success()
	{
		return Err(format!("ゲーム情報を取得できません: HTTP {}", response.status()));
	}
	let meta: crate::env::Meta = response.json().await.map_err(|error| error.to_string())?;
	if crate::env::normalize_game_id(&meta.id)? != game_id
	{
		return Err("Serverのゲーム情報のIDが要求と一致しません".into());
	}
	Ok(meta)
}
