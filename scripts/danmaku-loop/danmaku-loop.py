#!/usr/bin/env python3
"""画面のスクショを定期的に撮り、qwen-code に渡して弾幕を流すループ。

各ターンの流れ:
  1. スクショ保存先フォルダを空にする
  2. getscreens でスクショを撮り、JSON からパスを得る
  3. qwen を 1 回だけ呼ぶ (プロンプト先頭に @<path> を付けて画像を添付)
     コメント生成と danmaku send の実行は qwen 側が行う (--yolo)
  4. interval 秒待つ

qwen の終了コードや出力は見ない。あるターンで失敗しても淡々と次のターンへ進む。
プロンプト本文は同じフォルダの prompt.md を編集すれば差し替えられる。

OpenAI 互換 API の指定は次の優先順で解決する (上が強い):
  1. CLI 引数 (--base-url / --model / --api-key)
  2. 同階層の config.toml (あれば自動で読む)
どちらにも無ければ空文字で qwen に渡す。環境変数は設定ソースにしない
(空で渡し、qwen 自身の設定ファイル等には介入しない)。
qwen 起動時は --auth-type openai と値フラグを常に全部付ける。
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import time
import tomllib
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
PROMPT_FILE = SCRIPT_DIR / "prompt.md"
CONFIG_FILE = SCRIPT_DIR / "config.toml"


def screenshot_dir() -> Path:
    """スクショ保存先。getscreens のデフォルトと同じ場所に固定で揃える。"""
    base = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
    return Path(base) / "getscreens"


def take_screenshot(dir_: Path) -> str:
    """フォルダを空にしてから 1 枚撮り、その PNG パスを返す。"""
    shutil.rmtree(dir_, ignore_errors=True)
    dir_.mkdir(parents=True, exist_ok=True)
    out = subprocess.run(
        ["getscreens", "--dir", str(dir_)],
        capture_output=True,
        text=True,
        check=True,
    )
    entries = json.loads(out.stdout)
    return entries[0]["path"]


def run_qwen(image_path: str, prompt_body: str, opts: dict) -> None:
    """画像を添付してプロンプトを 1 回投げる。
    OpenAI 互換のフラグは常に全部付ける。指定が無い項目は空文字で渡す
    (空なら qwen がその回エラーを出す。終了コードは見ず次ターンへ進む)。
    qwen の stdout/stderr はそのまま端末に出る (隠さない)。
    """
    prompt = f"@{image_path}\n\n{prompt_body}"
    cmd = [
        "qwen", "--yolo", "-p", prompt,
        "--auth-type", "openai",
        "--openai-api-key", opts["api_key"],
        "--openai-base-url", opts["base_url"],
        "-m", opts["model"],
    ]
    try:
        subprocess.run(cmd, check=False)
    except Exception as e:  # qwen が無い等。次ターンへ進めるため止めない
        print(f"danmaku-loop: qwen 呼び出しに失敗: {e}", file=sys.stderr)


def load_config() -> dict:
    """同階層の config.toml を読む。無ければ空。壊れていれば警告して空。"""
    if not CONFIG_FILE.exists():
        return {}
    try:
        with open(CONFIG_FILE, "rb") as f:
            return tomllib.load(f)
    except (OSError, tomllib.TOMLDecodeError) as e:
        print(f"danmaku-loop: config.toml を無視 (読めない): {e}", file=sys.stderr)
        return {}


def resolve_opts(args, config: dict) -> dict:
    """qwen に渡す OpenAI 互換の値を解決する。
    優先順は 引数 > config.toml。どちらにも無ければ空文字。
    環境変数は設定ソースにしない (空で渡し、qwen 自身の設定には介入しない)。
    """
    def pick(arg_value, key):
        if arg_value:
            return arg_value
        return str(config.get(key) or "")

    return {
        "base_url": pick(args.base_url, "base_url"),
        "model": pick(args.model, "model"),
        "api_key": pick(args.api_key, "api_key"),
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="スクショを定期取得し qwen-code に弾幕を流させるループ"
    )
    parser.add_argument(
        "--interval", type=float, default=10.0,
        help="ターン間隔(秒)。デフォルト 10",
    )
    parser.add_argument(
        "--count", type=int, default=0,
        help="実行ターン数。0 または未指定で Ctrl-C まで無限",
    )
    parser.add_argument(
        "--base-url", default=None,
        help="qwen の --openai-base-url に渡す値。config.toml より優先",
    )
    parser.add_argument(
        "--model", default=None,
        help="qwen の -m に渡すモデル名。config.toml より優先",
    )
    parser.add_argument(
        "--api-key", default=None,
        help="qwen の --openai-api-key に渡す値。config.toml より優先",
    )
    args = parser.parse_args()

    prompt_body = PROMPT_FILE.read_text(encoding="utf-8")
    dir_ = screenshot_dir()
    opts = resolve_opts(args, load_config())

    turn = 0
    while args.count <= 0 or turn < args.count:
        turn += 1
        total = args.count if args.count > 0 else "∞"
        print(f"ターン {turn}/{total} 開始", flush=True)
        try:
            path = take_screenshot(dir_)
            run_qwen(path, prompt_body, opts)
        except Exception as e:  # getscreens 失敗等も次ターンへ
            print(f"danmaku-loop: ターン {turn} でエラー: {e}", file=sys.stderr)

        if args.count <= 0 or turn < args.count:
            time.sleep(args.interval)


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\ndanmaku-loop: 中断しました", file=sys.stderr)
