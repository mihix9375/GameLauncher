use serde::Serialize;

#[derive(Serialize)]
pub struct GameInfo
{
	id: String,
	title: String,
	version: String,
	image: String,
}

#[tauri::command]
pub fn reflesh() -> Vec<GameInfo>
{
	vec![
	GameInfo {
		id: "test1".to_string(),
		title: "Test1".to_string(),
		version: "v1.1.1".to_string(),
		image: "1".to_string(),
	},
	GameInfo {
		id: "test2".to_string(),
		title: "Test2".to_string(),
		version: "v1.2.3".to_string(),
		image: "2".to_string(),
	},
	]
}
