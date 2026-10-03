// Keep the original spelling for API requests; use the canonical form only for comparisons.
export function gameId(value) {
	const raw = value && typeof value === "object" ? value.id || value.game : value;
	return String(raw || "").trim().replace(/\.exe$/i, "");
}

export function canonicalGameId(value) {
	return gameId(value).toLocaleLowerCase();
}
