# danmaku-loop

画面のスクリーンショットを定期的に撮り、qwen-code に画像とプロンプトを渡して、画面内容へのニコニコ動画風の弾幕コメントを `danmaku send` で流させる定期実行スクリプト。

スクショ取得・コメント生成・送出の自律ループを、エージェントのスキルではなく単純な Python ループで回す。

## 前提

- `danmaku` がインストール済み（`which danmaku`）。serve は `danmaku send` が未起動時に自動起動するので、事前起動は不要
- `qwen` がインストール済み（`which qwen`）
- スクショ取得に OS 標準コマンドを使う（自作の `getscreens` は不要）:
  - **macOS**: `screencapture`（標準搭載）。**画面収録の権限**が必要 — システム設定 → プライバシーとセキュリティ → 画面収録 で、スクリプトを起動する端末アプリ（Terminal / iTerm / VS Code 等）を許可する。未許可だとエラーにならず**真っ黒な画像**が撮れる
  - **Linux (X11)**: `maim` と `xrandr`（`which maim` / `which xrandr`）

## 使い方

```
python3 danmaku-loop.py --interval 10 --count 6
```

| 引数 | 意味 | デフォルト |
|---|---|---|
| `--interval` | ターン間隔（秒） | 10 |
| `--count` | 実行ターン数。0 で Ctrl-C まで無限 | 0 |
| `--screen` | 撮影する画面番号（0 始まり、`danmaku --screen` と同番号体系）。今はメイン相当のみ | 0 |
| `--base-url` | qwen の `--openai-base-url` に渡す値 | （config.toml を使用） |
| `--model` | qwen の `-m` に渡すモデル名 | （config.toml を使用） |
| `--api-key` | qwen の `--openai-api-key` に渡す値 | （config.toml を使用） |

各ターンの流れ:

1. スクショ保存先フォルダを空にする
2. OS 標準コマンド（macOS: `screencapture` / Linux: `maim`+`xrandr`）でメイン画面を撮り、PNG パスを得る
3. `qwen` を 1 回呼ぶ（プロンプト先頭に `@<path>` を付けて画像を添付）。コメント生成と `danmaku send` の実行は qwen 側が行う
4. `--interval` 秒待つ

qwen の終了コードは見ない。あるターンで失敗してもメッセージは端末に表示され、そのまま次のターンへ進む。

## 設定ファイル `config.toml`

OpenAI 互換 API の接続先を `config.toml`（このディレクトリ直下）に書く。

```toml
base_url = "http://localhost:11434/v1"
model = "qwen2.5-vl"
api_key = "sk-..."
```

- 値の優先順位は **CLI 引数 > config.toml**。どちらにも無い項目は空文字で qwen に渡す。
- 環境変数は設定ソースにしない。
- qwen 起動時は `--auth-type openai` と値フラグを常に全部付ける。値が空のままだと qwen がその回エラーを出す。
- `config.toml` は API キーを含むため `.gitignore` 済み。

## プロンプト `prompt.md`

弾幕の指示文は `prompt.md` を編集すれば差し替えられる。スクリプトが先頭に `@<画像パス>` を付けて qwen に渡すので、本文に画像パスを書く必要はない。

## 派生スクリプト

同じループの実装違いが並んでいる。プロンプト（`prompt.md`）と `config.toml` の `interval` / `count` / `screen` は共通。

| スクリプト | コメント生成の担い手 | 接続先の指定 |
|---|---|---|
| `danmaku-loop-cline.py` | cline (エージェント) | `config.toml` の `base_url` / `api_key` / `model` |
| `danmaku-loop-openai.py` | OpenAI 互換 API を直接叩く | 同上 |
| `danmaku-loop-pi.py` | pi (エージェント) | `pi-agent/models.json`（接続先）+ `config.toml` の `model` |

### `danmaku-loop-pi.py`（pi 版）

cline 版と同じくエージェント（[pi](https://github.com/earendil-works/pi)）に画像を渡し、`bash` ツールで `danmaku send` を実行させる。違いは接続先の指定方法だけ:

- `pi` がインストール済み（`which pi`）。
- pi は base URL を実行毎フラグで受け取れないため、接続先は同階層 `pi-agent/models.json` にプロバイダとして定義しておく（起動時生成はしない）。スクリプトは `PI_CODING_AGENT_DIR` をこの `pi-agent` に向けて pi に読ませる。LM Studio を例にした雛形を同梱済み。
- `config.toml` の `model` は `models.json` の `id` と一致させる。`base_url` / `api_key` は `models.json` 側が持つ。
- 接続確認: `PI_CODING_AGENT_DIR="$PWD/pi-agent" pi --list-models` でモデルが出れば設定 OK。

```sh
python3 danmaku-loop-pi.py --interval 5 --count 20
```

`models.json` のプロバイダを変えたいときは `--provider`（既定 `lmstudio`）でも上書きできる。
