use std::io::Read;
use std::path::Path;

const MAX_DESCRIPTION_BYTES: u64 = 1024 * 1024;

/// Resolve a sibling .md file without changing the saved meta.json reference.
pub fn resolve(directory: &Path, value: &str) -> Result<String, String>
{
	let name = value.trim();
	if name.contains(['\n', '\r']) || !name.to_ascii_lowercase().ends_with(".md")
	{
		return Ok(value.to_string());
	}
	if name.contains(['/', '\\', ':']) || name == ".md"
	{
		return Err("説明ファイルはmeta.jsonと同じ階層に配置してください".into());
	}
	let directory = directory.canonicalize().map_err(|error| error.to_string())?;
	let path = directory.join(name).canonicalize().map_err(|_| format!("説明ファイルが見つかりません: {name}"))?;
	if path.parent() != Some(directory.as_path()) { return Err("説明ファイルがゲームフォルダー外を参照しています".into()); }
	let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
	let mut bytes = Vec::new();
	file.take(MAX_DESCRIPTION_BYTES + 1).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
	if bytes.len() as u64 > MAX_DESCRIPTION_BYTES { return Err("説明ファイルは1 MiB以内にしてください".into()); }
	let text = String::from_utf8(bytes).map_err(|_| "説明ファイルはUTF-8で保存してください".to_string())?;
	Ok(text.trim_start_matches('\u{feff}').to_string())
}

#[cfg(test)]
mod tests
{
	use super::*;

	#[test]
	fn resolves_markdown_and_rejects_outside_paths()
	{
		let root = std::env::temp_dir().join(format!("launcher-description-{}", std::process::id()));
		std::fs::create_dir_all(&root).unwrap();
		std::fs::write(root.join("README.md"), "\u{feff}# Overview\n\n- Play").unwrap();
		assert_eq!(resolve(&root, "README.md").unwrap(), "# Overview\n\n- Play");
		assert_eq!(resolve(&root, "# Overview\nREADME.md").unwrap(), "# Overview\nREADME.md");
		assert!(resolve(&root, "../README.md").is_err());
		assert!(resolve(&root, "missing.md").is_err());
		std::fs::remove_dir_all(root).unwrap();
	}
}
