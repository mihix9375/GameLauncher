use serde::Serialize;
use tonic::Request;

use crate::env::gamelauncher::{AddCommentRequest, Comment, CommentListRequest};

#[derive(Serialize)]
pub struct SerializedComment
{
	pub id: String,
	pub game_id: String,
	pub author: String,
	pub content: String,
	pub created_at: i64,
}

impl From<Comment> for SerializedComment
{
	fn from(value: Comment) -> Self
	{
		Self {
			id: value.id,
			game_id: value.game_id,
			author: value.author,
			content: value.content,
			created_at: value.created_at,
		}
	}
}

#[tauri::command]
pub async fn list_comments(game_id: String) -> Result<Vec<SerializedComment>, String>
{
	let game_id = crate::env::normalize_game_id(&game_id)?;
	let url = crate::env::get_config().server_url;
	let mut client = crate::env::connect_and_get_client(url);
	let response = client.list_comments(Request::new(CommentListRequest { game_id }))
		.await
		.map_err(|error| error.message().to_string())?;
	Ok(response.into_inner().comments.into_iter().map(SerializedComment::from).collect())
}

#[tauri::command]
pub async fn add_comment(
	game_id: String,
	author: String,
	content: String,
) -> Result<SerializedComment, String>
{
	let game_id = crate::env::normalize_game_id(&game_id)?;
	let url = crate::env::get_config().server_url;
	let mut client = crate::env::connect_and_get_client(url);
	let response = client.add_comment(Request::new(AddCommentRequest {
		game_id,
		author,
		content,
	}))
		.await
		.map_err(|error| error.message().to_string())?;
	Ok(SerializedComment::from(response.into_inner()))
}
