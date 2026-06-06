# danmaku-loop

画面のスクリーンショットを定期的に撮り、qwen-code に画像とプロンプトを渡して、画面内容へのニコニコ動画風の弾幕コメントを `danmaku send` で流させる定期実行スクリプト。

スクショ取得・コメント生成・送出の自律ループを、エージェントのスキルではなく単純な Python ループで回す。

## 前提

- `getscreens` がインストール済み（`which getscreens`）
- `danmaku` がインストール済み（`which danmaku`）。serve は `danmaku send` が未起動時に自動起動するので、事前起動は不要
- `qwen` がインストール済み（`which qwen`）

## 使い方

```
python3 danmaku-loop.py --interval 10 --count 6
```

| 引数 | 意味 | デフォルト |
|---|---|---|
| `--interval` | ターン間隔（秒） | 10 |
| `--count` | 実行ターン数。0 で Ctrl-C まで無限 | 0 |
| `--base-url` | qwen の `--openai-base-url` に渡す値 | （config.toml を使用） |
| `--model` | qwen の `-m` に渡すモデル名 | （config.toml を使用） |
| `--api-key` | qwen の `--openai-api-key` に渡す値 | （config.toml を使用） |

各ターンの流れ:

1. スクショ保存先フォルダを空にする
2. `getscreens` でスクショを撮り、PNG パスを得る
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
