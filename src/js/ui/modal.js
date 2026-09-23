export function openModal(modalId) {
	const modal = document.getElementById(modalId);
	if (modal) {
		modal.classList.remove("hidden");
	}
}

export function closeModal(modalId) {
	const modal = document.getElementById(modalId);
	if (modal) {
		modal.classList.add("hidden");
		modal.dispatchEvent(new CustomEvent("modal:closed", {
			bubbles: true,
			detail: { modalId },
		}));
	}
}
