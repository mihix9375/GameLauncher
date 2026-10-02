const stageLabels = {
	queued: "ダウンロード待機中",
	preparing: "準備中",
	extracting: "展開中",
	installing: "適用中",
};

export function downloadProgress(game) {
	const progress = game?._downloadProgress;
	const measured = progress?.total_bytes > 0;
	const percentage = measured
		? Math.min(100, Math.round((progress.received_bytes / progress.total_bytes) * 100))
		: 0;
	return {
		downloading: Boolean(game?._isDownloading && progress),
		measured,
		percentage,
		label: stageLabels[progress?.stage] || `${percentage}% ダウンロード中`,
	};
}

export function updateProgressBar(wrapper, bar, progress) {
	if (wrapper) {
		wrapper.classList.toggle("indeterminate", !progress.measured);
		if (progress.measured) wrapper.setAttribute("aria-valuenow", String(progress.percentage));
		else wrapper.removeAttribute("aria-valuenow");
	}
	if (bar) bar.style.width = `${progress.percentage}%`;
}
