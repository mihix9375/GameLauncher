import { invoke } from "../core/tauri.js";
import { findGameById, getAllGames, setAllGames } from "../core/state.js";
import { renderGames } from "./renderGames.js";
import { updateGameCount } from "../ui/counter.js";
import { setLogText } from "../ui/log.js";
import { gameId, canonicalGameId } from "../core/gameIdentity.js";

let latestLoadRequest = 0;

function normalizeGame(game) {
	game.id = gameId(game);
	if (!game.game && game.id) game.game = `${game.id}.exe`;
	if (!game.image && game.titleImage) game.image = game.titleImage;
	if (!game.titleImage && game.image) game.titleImage = game.image;
	if (game.isInstalled === undefined) game.isInstalled = true;
	if (game.hasUpdate === undefined) game.hasUpdate = false;
	return game;
}

export function sortGames(games) {
	return games.sort((left, right) => {
		const titleOrder = String(left.title || left.id || "").localeCompare(
			String(right.title || right.id || ""),
			"ja",
			{ numeric: true, sensitivity: "base" },
		);
		return titleOrder || canonicalGameId(left).localeCompare(canonicalGameId(right));
	});
}

export async function refreshGameCard(gameId) {
	if (!window.__TAURI__) return findGameById(gameId);
	const refreshedGames = (await invoke("refresh")).map(normalizeGame);
	const freshGame = refreshedGames.find(game => canonicalGameId(game) === canonicalGameId({ id: gameId }));
	if (!freshGame) return null;

	const existing = findGameById(gameId);
	if (existing) {
		Object.assign(existing, freshGame, {
			isInstalled: true,
			hasUpdate: false,
			_needsUpdate: false,
		});
		delete existing._latestVersion;
		delete existing._downloadError;
	} else {
		getAllGames().push(freshGame);
	}

	sortGames(getAllGames());
	renderGames(getAllGames());
	updateGameCount(getAllGames().length);
	return existing || freshGame;
}

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

		allGames.forEach(normalizeGame);

		// refreshはローカルゲームだけを返すため、Serverから通知された未導入カードを維持する。
		const localIds = new Set(allGames.map(canonicalGameId));
		for (const remoteGame of getAllGames().filter(game => game.isInstalled === false)) {
			const remoteId = canonicalGameId(remoteGame);
			if (remoteId && !localIds.has(remoteId)) allGames.push(remoteGame);
		}

		sortGames(allGames);
		setAllGames(allGames);
		renderGames(allGames);
		updateGameCount(allGames.length);
		setLogText(`準備完了 - ${allGames.length}件のゲームを読み込みました`);
	} catch (error) {
		if (requestNumber !== latestLoadRequest) return;
		console.error("Failed to load games:", error);
		setLogText(`エラー: ゲーム情報の取得に失敗しました (${error})`);
	}
}
