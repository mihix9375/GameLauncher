export function setCommunityTab(tabName) {
	const commentsTab = document.getElementById("tab-comments");
	const rankingTab = document.getElementById("tab-ranking");
	const commentsPanel = document.getElementById("comment-section");
	const rankingPanel = document.getElementById("leaderboard-section");
	const showRanking = tabName === "ranking" && rankingTab && !rankingTab.hidden;

	commentsTab?.classList.toggle("active", !showRanking);
	commentsTab?.setAttribute("aria-selected", String(!showRanking));
	commentsTab?.setAttribute("tabindex", showRanking ? "-1" : "0");
	if (commentsPanel) commentsPanel.hidden = showRanking;

	rankingTab?.classList.toggle("active", Boolean(showRanking));
	rankingTab?.setAttribute("aria-selected", String(Boolean(showRanking)));
	rankingTab?.setAttribute("tabindex", showRanking ? "0" : "-1");
	if (rankingPanel) rankingPanel.hidden = !showRanking;
}
