fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = tonic_prost_build::compile_protos("proto/gamelauncher.proto");

	tauri_build::build();

	Ok(())
}
