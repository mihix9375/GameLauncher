const closeButton = document.getElementById("close-game");

closeButton?.addEventListener("click", async () => {
  closeButton.disabled = true;
  try {
    await window.__TAURI__.core.invoke("close_game");
  } catch (error) {
    console.error("Failed to close game:", error);
	} finally {
    closeButton.disabled = false;
  }
});
