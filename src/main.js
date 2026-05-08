const { invoke } = window.__TAURI__.core;

async function loadGames()
{
	try {
		const games = await invoke("reflesh");
		const container = document.getElementById("game-list");
		container.innerHTML = "";

		games.forEach(game => {
			const card = document.createElement("div");
			card.className = "game-card";

			const title = document.createElement("h3");
			title.textContent = game.title;

			const version = document.createElement("p");
			version.textContent = game.version;

			const image = document.createElement("p");
			image.textContent = game.image;

			const btn = document.createElement("button");
			btn.className = "btn primary";
			btn.textContent = "起動する";

			btn.addEventListener("click", async () => {
				await launch(game.id);
			});

			card.appendChild(image);
			card.appendChild(title);
			card.appendChild(version);
			card.appendChild(btn);

			container.appendChild(card);
		});
	}
	catch (error)
	{
		console.error("ゲーム情報の表示に失敗しました。");
		alert("ゲーム情報の表示に失敗しました。");
	}
}

async function launch(id)
{
	try {
		const result = await invoke("launch");

		console.log(result);
		alert(result);
	}
	catch (error)
	{
		console.error("エラー: ", error);
	}
}

loadGames();
