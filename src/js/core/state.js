import { canonicalGameId } from "./gameIdentity.js";

let allGames = [];
let selectedGame = null;
let runningGameId = "";

export function setRunningGameId(id) {
	runningGameId = canonicalGameId(id);
}

export function isGameRunning(game) {
	return Boolean(runningGameId) && canonicalGameId(game) === runningGameId;
}

const MOCK_METADATA = {
	default: {
		description: "ゲーム開発研究部によって制作されたオリジナル作品です。圧倒的な世界観と爽快なゲームプレイ、細部までこだわり抜かれたグラフィックスをお楽しみください。",
		author: "ゲーム開発研究部",
		lastUpdate: "2026/07/07",
		tags: ["3D", "アクション", "部内作品"]
	},
	test1: {
		description: "ハイスピードな3Dアクションと爽快なコンボシステムが特徴のフラッグシップタイトル。最新のシェーダーエフェクトとレスポンシブな操作性を実現しています。",
		author: "白石＆開発チーム",
		lastUpdate: "2026/07/01",
		tags: ["3Dアクション", "コンボ", "フラッグシップ"]
	},
	test2: {
		description: "戦略的な思考と素早い判断が求められる新感覚タクティカルゲーム。マルチプレイやオンラインランキングにも対応予定の注目の新作です。",
		author: "開発研究部 Alpha隊",
		lastUpdate: "2026/07/05",
		tags: ["タクティカル", "戦略", "マルチプレイ対応"]
	}
};

export function getAllGames() {
	return allGames;
}

export function setAllGames(games) {
	allGames = games;
}

export function findGameById(gameId) {
	const targetId = canonicalGameId(gameId);
	return allGames.find(game => canonicalGameId(game) === targetId);
}

export function removeGameById(gameId) {
	const targetId = canonicalGameId(gameId);
	if (!targetId) return false;
	const previousLength = allGames.length;
	allGames = allGames.filter(game => {
		const id = canonicalGameId(game);
		return id !== targetId;
	});
	if (selectedGame && canonicalGameId(selectedGame) === targetId) {
		selectedGame = null;
	}
	return allGames.length !== previousLength;
}

export function getSelectedGame() {
	return selectedGame;
}

export function setSelectedGame(game) {
	selectedGame = game;
}

export function getMockMetadata() {
	return MOCK_METADATA;
}

export function setGameUpdateFlag(gameId, version) {
	const game = findGameById(gameId);
	if (game) {
		game.hasUpdate = true;
		game._needsUpdate = true;
		if (version) game._latestVersion = version;
	} else {
		allGames.push({
			id: gameId,
			game: gameId,
			title: "ゲーム情報を取得中…",
			version: "未インストール",
			isInstalled: false,
			hasUpdate: true,
			_needsUpdate: true,
			_latestVersion: version,
			image: "3"
		});
	}
}

export function applyRemoteGameMetadata(gameId, metadata) {
	const game = findGameById(gameId);
	if (!game || game.isInstalled !== false || !metadata || canonicalGameId(metadata) !== canonicalGameId(gameId)) return null;
	// Update descriptive data without resetting download progress or update flags.
	Object.assign(game, {
		title: metadata.title || "タイトル未設定",
		author: metadata.author || "",
		description: metadata.description || "",
		tags: metadata.tags || [],
		game: metadata.game || game.game,
		latestUpdate: metadata.latestUpdate || metadata.latest_update || "",
		version: metadata.version || game.version,
	});
	const image = metadata.titleImage || metadata.title_image;
	if (image) {
		game.image = image;
		game.titleImage = image;
	}
	return game;
}

export function setGameDownloadProgress(gameId, progress) {
	const game = findGameById(gameId);
	if (!game) return false;
	if (!progress || progress.stage === "complete") {
		delete game._downloadProgress;
		game._isDownloading = false;
		return true;
	}
	if (progress.stage === "error") {
		delete game._downloadProgress;
		game._isDownloading = false;
		game._downloadError = progress.error || "ダウンロードに失敗しました";
		return true;
	}
	game._downloadProgress = progress;
	game._isDownloading = true;
	delete game._downloadError;
	return true;
}
