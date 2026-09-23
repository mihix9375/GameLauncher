export function formatError(error, fallback = "不明なエラーが発生しました") {
	if (typeof error === "string" && error.trim()) return error.trim();
	if (error?.message && String(error.message).trim()) return String(error.message).trim();
	try {
		const serialized = JSON.stringify(error);
		if (serialized && serialized !== "{}" && serialized !== "null") return serialized;
	} catch {
		// Fall through to the human-readable fallback.
	}
	return fallback;
}
