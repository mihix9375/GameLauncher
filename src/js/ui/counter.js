export function updateGameCount(count) {
	const countEl = document.getElementById("game-count");
	if (countEl) {
		countEl.textContent = `(${count})`;
	}
}
