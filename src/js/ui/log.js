export function setLogText(text) {
	const logEl = document.getElementById("log-text");
	if (logEl) {
		logEl.textContent = text;
	}
}
