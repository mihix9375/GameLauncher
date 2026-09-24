const scrollHintControllers = new WeakMap();

/**
 * スクロール領域の下に続きがある間だけ、案内ボタンを表示します。
 */
export function setupScrollHint(scroller, hint, options = {}) {
	if (!scroller || !hint) return;

	const existing = scrollHintControllers.get(scroller);
	if (existing) {
		existing.update();
		return;
	}

	const {
		minStep = 180,
		stepRatio = 0.72,
		observedElements = [],
	} = options;

	const update = () => {
		const remaining = scroller.scrollHeight - scroller.clientHeight - scroller.scrollTop;
		const hasOverflow = scroller.scrollHeight > scroller.clientHeight + 4;
		hint.hidden = !hasOverflow || remaining <= 8;
	};

	scroller.addEventListener("scroll", update, { passive: true });
	hint.addEventListener("click", () => {
		scroller.scrollBy({
			top: Math.max(minStep, scroller.clientHeight * stepRatio),
			behavior: "smooth",
		});
	});
	window.addEventListener("resize", update, { passive: true });

	if (typeof ResizeObserver !== "undefined") {
		const resizeObserver = new ResizeObserver(update);
		resizeObserver.observe(scroller);
		observedElements.filter(Boolean).forEach(element => resizeObserver.observe(element));
	}

	scrollHintControllers.set(scroller, { update });
	requestAnimationFrame(update);
}
