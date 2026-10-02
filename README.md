# GameLauncher

部内LAN上のGameServerからゲームを取得・更新・起動するWindows向けランチャーです。Tauri 2、Rust、HTML/CSS/JavaScriptで構成されています。

## 主な機能

- GameServerからゲーム一覧とバージョンを取得
- ゲームのインストール、更新、起動
- 更新通知によるインストール済みゲームの自動更新
- 専用の一括ダウンロードボタンによる未導入ゲームを含む全ゲームの取得
- ゲームカード上でのダウンロード進捗表示
- SHA-256マニフェストによるファイル単位の差分更新
- GameServerで配布終了したゲームの自動削除
- 初回インストールと大規模更新時の完全ZIPダウンロード
- 最大4ワーカーでのZIP並列展開
- ゲーム別コメントの表示と投稿
- コメントと最大2つのランキングをタブで表示
- ゲーム画像、説明、作者、バージョンをまとめた詳細画面
- Unityゲーム向けローカルランキングAPIの提供
- UnityのF11フルスクリーン上でも使える終了オーバーレイ
- F11キーでLauncherウィンドウのフルスクリーンを切り替え
- 多重起動の抑止と、Launcher終了時のゲーム・補助処理の終了

## 全体構成

```text
Unityゲーム
  └─ GameLauncher-Unity-Ranking
       └─ GameLauncher :50053（ローカルHTTP API）
            ├─ GameServer :50050（ゲーム、更新、コメント）
            └─ GameServer :50052（ランキング）
```

UnityゲームはGameServerへ直接接続しません。ServerのIPアドレスはLauncherで一度設定し、ゲームは常に同じPCのLauncherへ接続します。

## 通信構成

| 通信 | 既定の接続先 | 用途 |
|---|---|---|
| Launcher → GameServer | `http://[::1]:50050` | gRPCによる一覧、コメント、更新、ゲーム配信 |
| Launcher → Ranking API | `http://127.0.0.1:50052` | ランキングの取得・投稿 |
| Unity → Launcher | `http://127.0.0.1:50053` | Unity向けローカルランキングAPI |

別PCのGameServerへ接続する場合は、Launcherの設定画面でServerのIPアドレスを指定します。

```text
ゲームServer:        http://192.168.1.10:50050
ランキングServer API: http://192.168.1.10:50052
```

## 利用方法

1. GameServerを起動します。
2. GameLauncherを起動します。
3. 設定画面でServer URL、ランキングServer API、ゲーム保存先を設定します。
4. 一覧からゲームを選び、インストールまたは起動します。

未ダウンロードのゲームもServerからゲーム情報を取得し、カードと詳細画面にタイトルを表示します。この情報取得でゲーム本体はダウンロードされません。接続先は設定したランキングServer APIです。

設定は `%APPDATA%\gamelauncher\config.json` に保存されます。ゲーム保存先を空欄にした場合は `%APPDATA%\gamelauncher\games` を使用します。

ゲームの説明は空行を含む改行に対応しています。Server管理画面の説明欄ではEnterで改行し、`meta.json`を直接編集する場合は`"description": "ゲームの説明\n\n操作方法\nWASD: 移動"`のように`\n`を使ってください。Launcherの詳細画面では、その改行と空行を保って表示します。

説明はMarkdownでも記述できます。見出し、太字、箇条書き、表、引用、コード、リンクを表示できます。`"description": "README.md"`と指定すると、`meta.json`と同じ階層にあるUTF-8のMDファイルを読み込みます。ファイルは1 MiB以内で、ゲームZIPに含めてください。画像はHTTPS URLに対応し、HTMLタグは文字として表示します。

## ゲーム更新

Serverから更新通知を受け取った場合、すでにインストールされているゲームだけを自動更新します。未導入ゲームは一覧へ追加されますが、自動ではダウンロードされません。個別のダウンロードボタン、または画面右下の一括ダウンロードボタンから明示的に取得できます。隣の更新ボタンはゲーム情報の再読込に使用します。一括取得では最大4ゲームを同時に転送します。ダウンロード中もゲームカードは一覧に残り、ゲームカードと詳細画面に受信・展開・適用の進捗が表示されます。完了したカードは`meta.json`と画像を再読込してタイトル順に並べ直されます。個別ダウンロードや更新が完了してもゲームは自動起動しません。

更新時はServerのマニフェストとインストール済みファイルを比較し、追加・変更されたファイルだけを2 MiB単位で取得します。変更されていないファイルは再利用され、新しいマニフェストに存在しないファイルは削除されます。

変更ファイルが複数ある場合はサイズが偏らないよう最大4グループに分け、1本のHTTP/2接続上で複数のgRPCストリームを同時に転送します。受信とディスク書き込みも有界キューで分離されます。

変更ファイルの非圧縮合計が配布ZIPのサイズ以上になる場合は、通信量を抑えるため完全ZIPへ自動的に切り替えます。受信後はサイズとSHA-256を検証し、更新に失敗した場合は以前のバージョンへ戻します。

GameServerの管理画面でゲームが削除されると、Launcher側のインストール済みゲームも削除されます。Launcherがオフラインだった場合は、同じServerへの次回接続時に削除履歴が同期されます。対象ゲームが起動中なら、そのゲームを終了してから削除します。

## UnityランキングAPI

ゲームはGameServerへ直接接続せず、起動中のLauncherが提供するローカルAPIを使用します。

Unity Package Managerから専用パッケージを追加すると、HTTPやJSONを直接実装せずに利用できます。

> [!IMPORTANT]
> ランキングAPI v0.2.0は旧APIと後方互換性がありません。`game_id`や自由文字列のランキングIDを指定する方式は廃止され、GameLauncherが発行するセッショントークンと固定スロット`0`・`1`を使用します。GameLauncher v0.6.0、GameServer v0.5.0、UPMパッケージ v0.2.0を組み合わせてください。

```text
https://github.com/mihix9375/GameLauncher-Unity-Ranking.git#v0.2.0
```

```csharp
using GameLauncher.Ranking;

var ranks = await RankingApi.SyncLeaderboardsAsync();
await ranks[0].SetAsync("ハイスコア", RankingOrder.HighScore);
await ranks[0].EnableAsync();
ScoreResult result = await ranks[0].InsertAsync(playerName, score);
```

詳しい導入方法とサンプルは [GameLauncher-Unity-Ranking](https://github.com/mihix9375/GameLauncher-Unity-Ranking) を参照してください。

パッケージを使用しない場合は、次のHTTP APIを直接呼び出すこともできます。

```http
GET http://127.0.0.1:50053/v1/leaderboards
Authorization: Bearer {GameLauncherが起動時に渡すトークン}
```

```http
POST http://127.0.0.1:50053/v1/leaderboards/{slot}/scores
Authorization: Bearer {GameLauncherが起動時に渡すトークン}
Content-Type: application/json

{"player_name":"PLAYER","score":12000}
```

ゲーム側の定義配列を同期する場合は`PUT /v1/leaderboards`を使います。配列位置がランキングスロット`0`・`1`になり、スコア送信もこの番号だけを指定します。ゲームIDはどのAPIにも指定せず、Launcherが起動時に発行したセッショントークンから対象ゲームを内部で確定します。

応答形式などの低水準仕様はGameServerの [UNITY_LEADERBOARD_API.md](https://github.com/mihix9375/GameServer/blob/main/UNITY_LEADERBOARD_API.md) を参照してください。Launcherを終了するとローカルAPIも終了します。

## 開発環境

- Windows 10/11
- Node.jsおよびnpm
- Rust stable（MSVC toolchain）
- Microsoft C++ Build Tools
- WebView2 Runtime

protoはGit submoduleです。最初にsubmoduleを含めて取得してください。

```powershell
git clone --recursive https://github.com/mihix9375/GameLauncher.git
cd GameLauncher
npm install
npm run tauri dev
```

submoduleなしでcloneした場合は次を実行します。

```powershell
git submodule update --init --recursive
```

## ビルド

```powershell
npm install
npm run tauri build
```

生成物は通常 `src-tauri\target\release\bundle` 以下に出力されます。

## テスト

```powershell
cd src-tauri
cargo test
```

## protoの更新

共有定義は `src-tauri/proto` submoduleで管理しています。protoを変更するときは共有protoリポジトリへ先にpushし、このリポジトリではsubmodule参照を更新してコミットしてください。GameServer側では同じ共有protoをpullして使用します。

## 関連リポジトリ

- [GameServer](https://github.com/mihix9375/GameServer)
- [GameLauncher Ranking API for Unity](https://github.com/mihix9375/GameLauncher-Unity-Ranking)
- [共有proto](https://github.com/mihix9375/proto)
