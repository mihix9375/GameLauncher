export function openModal(modalId) {
	const modal = document.getElementById(modalId);
	if (modal) {
		const animationToken = {};
		modal._openAnimationToken = animationToken;
		modal.classList.add("opening");
		modal.inert = false;
		modal.setAttribute("aria-hidden", "false");
		modal.classList.remove("hidden");
		window.requestAnimationFrame(() => {
			if (modal._openAnimationToken === animationToken) {
				document.body.classList.add("modal-background-blurred");
			}
			window.requestAnimationFrame(() => {
				if (modal._openAnimationToken === animationToken) modal.classList.remove("opening");
			});
		});
	}
}

export function closeModal(modalId) {
	const modal = document.getElementById(modalId);
	if (modal) {
		modal._openAnimationToken = null;
		modal.classList.remove("opening");
		if (modal.contains(document.activeElement)) {
			document.activeElement.blur();
		}
		modal.classList.add("hidden");
		modal.inert = true;
		modal.setAttribute("aria-hidden", "true");
		if (!document.querySelector(".modal-overlay:not(.hidden)")) {
			document.body.classList.remove("modal-background-blurred");
		}
		modal.dispatchEvent(new CustomEvent("modal:closed", {
			bubbles: true,
			detail: { modalId },
		}));
	}
}
