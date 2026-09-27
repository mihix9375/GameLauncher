const closeButton = document.getElementById("close-game");
const confirmation = document.getElementById("close-confirmation");
const cancelButton = document.getElementById("confirm-cancel");
const confirmButton = document.getElementById("confirm-close");

window.showCloseConfirmation = () => {
  document.body.classList.add("is-confirming");
  confirmation?.setAttribute("aria-hidden", "false");
  window.setTimeout(() => cancelButton?.focus(), 0);
};

window.hideCloseConfirmation = () => {
  document.body.classList.remove("is-confirming");
  confirmation?.setAttribute("aria-hidden", "true");
};

confirmButton?.addEventListener("click", async () => {
  confirmButton.disabled = true;
  cancelButton.disabled = true;
  try {
    await window.__TAURI__.core.invoke("close_game");
  } catch (error) {
    console.error("Failed to close game:", error);
	} finally {
    confirmButton.disabled = false;
    cancelButton.disabled = false;
  }
});

cancelButton?.addEventListener("click", async () => {
  cancelButton.disabled = true;
  try {
    await window.__TAURI__.core.invoke("cancel_close_game");
  } catch (error) {
    console.error("Failed to cancel closing the game:", error);
  } finally {
    cancelButton.disabled = false;
  }
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && document.body.classList.contains("is-confirming")) {
    cancelButton?.click();
  }
});
