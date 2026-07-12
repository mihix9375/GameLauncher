let allGames = [];
let selectedGame = null;

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
	const game = allGames.find(g => g.id === gameId || g.game === gameId);
	if (game) {
		game.hasUpdate = true;
		game._needsUpdate = true;
		if (version) game._latestVersion = version;
	} else {
		allGames.push({
			id: gameId,
			game: gameId,
			title: gameId,
			version: "未インストール",
			isInstalled: false,
			hasUpdate: true,
			_needsUpdate: true,
			_latestVersion: version,
			image: "3"
		});
	}
}
