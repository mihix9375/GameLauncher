export function setupFullscreenToggle() {
	const appWindow = window.__TAURI__?.window?.getCurrentWindow();
	if (!appWindow) return;

	document.addEventListener("keydown", async (event) => {
		if (event.key !== "F11" || event.repeat) return;
		event.preventDefault();
		try {
			await appWindow.setFullscreen(!(await appWindow.isFullscreen()));
		} catch (error) {
			console.error("フルスクリーンの切り替えに失敗しました", error);
		}
	});
}
