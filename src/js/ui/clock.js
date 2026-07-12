export function initClock() {
	const timeEl = document.getElementById("log-time");
	if (!timeEl) return;

	const update = () => {
		const now = new Date();
		const hours = String(now.getHours()).padStart(2, '0');
		const mins = String(now.getMinutes()).padStart(2, '0');
		const secs = String(now.getSeconds()).padStart(2, '0');
		timeEl.textContent = `${hours}:${mins}:${secs}`;
	};

	update();
	setInterval(update, 1000);
}
