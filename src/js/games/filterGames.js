import { getAllGames, getMockMetadata } from "../core/state.js";
import { renderGames } from "./renderGames.js";
import { updateGameCount } from "../ui/counter.js";

export function filterAndRenderGames(query) {
	const allGames = getAllGames();
	const q = query.trim().toLowerCase();
	if (!q) {
		renderGames(allGames);
		updateGameCount(allGames.length);
		return;
	}

	const MOCK_METADATA = getMockMetadata();
	const filtered = allGames.filter(game => {
		const meta = MOCK_METADATA[game.id] || MOCK_METADATA.default;
		const titleMatch = game.title.toLowerCase().includes(q);
		const tagMatch = meta.tags.some(tag => tag.toLowerCase().includes(q));
		return titleMatch || tagMatch;
	});

	renderGames(filtered);
	updateGameCount(filtered.length);
}
