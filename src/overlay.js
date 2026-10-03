const closeButton = document.getElementById("close-game");
const confirmation = document.getElementById("close-confirmation");
const cancelButton = document.getElementById("confirm-cancel");
const confirmButton = document.getElementById("confirm-close");

function reportOverlayError(message, error) {
  console.error(message, error);
  const detail = error instanceof Error ? error.stack || error.message : String(error);
  void window.__TAURI__.core.invoke("write_client_log", {
    level: "ERROR", source: "overlay", message: `${message}: ${detail}`.slice(0, 16000),
  }).catch(() => {});
}

function waitForHiddenFrame() {
  return new Promise(resolve => {
    window.requestAnimationFrame(() => window.requestAnimationFrame(resolve));
  });
}

window.showCloseConfirmation = () => {
  document.body.classList.remove("is-positioning");
  document.body.classList.add("is-confirming");
  confirmation?.setAttribute("aria-hidden", "false");
  window.setTimeout(() => cancelButton?.focus(), 0);
};

window.hideCloseConfirmation = () => {
  document.body.classList.remove("is-confirming");
  confirmation?.setAttribute("aria-hidden", "true");
};

closeButton?.addEventListener("click", async () => {
  if (closeButton.disabled) return;
  closeButton.disabled = true;
  document.body.classList.add("is-positioning");
  document.body.classList.remove("overlay-ready");
  await waitForHiddenFrame();
  try {
    await window.__TAURI__.core.invoke("request_close_game");
  } catch (error) {
    reportOverlayError("Failed to show close confirmation", error);
  } finally {
    closeButton.disabled = false;
  }
});

confirmButton?.addEventListener("click", async () => {
  confirmButton.disabled = true;
  cancelButton.disabled = true;
  try {
    await window.__TAURI__.core.invoke("close_game");
  } catch (error) {
    reportOverlayError("Failed to close game", error);
	} finally {
    confirmButton.disabled = false;
    cancelButton.disabled = false;
  }
});

cancelButton?.addEventListener("click", async () => {
  cancelButton.disabled = true;
  document.body.classList.add("is-positioning");
  window.hideCloseConfirmation?.();
  await waitForHiddenFrame();
  try {
    await window.__TAURI__.core.invoke("cancel_close_game");
  } catch (error) {
    reportOverlayError("Failed to cancel closing the game", error);
    window.showCloseConfirmation?.();
  } finally {
    cancelButton.disabled = false;
  }
});

document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && document.body.classList.contains("is-confirming")) {
    cancelButton?.click();
  }
});
