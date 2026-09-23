import { invoke } from "../core/tauri.js";
import { setAllGames } from "../core/state.js";
import { renderGames } from "./renderGames.js";
import { updateGameCount } from "../ui/counter.js";
import { setLogText } from "../ui/log.js";

let latestLoadRequest = 0;

export async function loadGames() {
	const requestNumber = ++latestLoadRequest;
	try {
		setLogText("ゲーム情報を取得中...");
		let allGames;

		if (window.__TAURI__) {
			allGames = await invoke("refresh");
		} else {
			allGames = [
				{ id: "test1", title: "Project Alpha: Overdrive", version: "v1.1.1", image: "1" },
				{ id: "test2", title: "Cyber Horizon 2026", version: "v1.2.3", image: "2" },
				{ id: "test3", title: "Dungeon Chronicle", version: "v0.9.5", image: "1" },
				{ id: "test4", title: "Skyward Wings", version: "v2.0.0", image: "2" },
				{ id: "test5", title: "Mech Arena: Zero", version: "v1.0.4", image: "1" },
				{ id: "test6", title: "Rhythm Master Pro", version: "v3.1.0", image: "2" }
			];
		}
		if (requestNumber !== latestLoadRequest) return;

		allGames.forEach(g => {
			if (g.id && g.id.endsWith(".exe")) g.id = g.id.replace(".exe", "");
			if (!g.id && g.game) g.id = g.game.replace(".exe", "");
			if (!g.game && g.id) g.game = `${g.id}.exe`;
			if (!g.image && g.titleImage) g.image = g.titleImage;
			if (!g.titleImage && g.image) g.titleImage = g.image;
			if (g.isInstalled === undefined) g.isInstalled = true;
			if (g.hasUpdate === undefined) g.hasUpdate = false;
		});

		setAllGames(allGames);
		renderGames(allGames);
		updateGameCount(allGames.length);
		setLogText(`準備完了 - ${allGames.length}件のゲームを読み込みました`);
	} catch (error) {
		if (requestNumber !== latestLoadRequest) return;
		console.error("Failed to load games:", error);
		setLogText(`エラー: ゲーム情報の取得に失敗しました (${error})`);
		alert(`ゲーム情報の表示に失敗しました。\n詳細: ${error}`);
	}
}
