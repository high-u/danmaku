#!/usr/bin/env python3
"""画面のスクショを定期的に撮り、OpenAI 互換 API に投げて弾幕を流すループ。

cline 版 (danmaku-loop.py) と違い、エージェント (cline) を介さない。
責務を分離する:
  - AI (OpenAI 互換 API): 画像を見て弾幕コメントを「改行区切りのテキスト」で返すだけ
  - この Python: スクショ撮影・API 呼び出し・danmaku コマンドの実行 を全部やる

各ターンの流れ:
  1. スクショ保存先フォルダを空にする
  2. OS ごとの素のコマンドでスクショを撮り、その PNG パスを得る
  3. 画像を base64 化し、prompt-openai.md を添えて /v1/chat/completions を 1 回呼ぶ
  4. 返ってきたテキストを改行で分割し、各行を 1 コメントとして `danmaku send` を実行
  5. interval 秒待つ

あるターンで失敗しても淡々と次のターンへ進む。
プロンプト本文は同階層の prompt-openai.md を編集すれば差し替えられる。

OpenAI 互換 API の指定は次の優先順で解決する (上が強い):
  1. CLI 引数 (--base-url / --model / --api-key)
  2. 同階層の config.toml (あれば自動で読む)
HTTP は標準ライブラリ (urllib) のみで叩く。外部依存は無い。
"""

import argparse
import base64
import json
import os
import shutil
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request
from datetime import datetime
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
PROMPT_FILE = SCRIPT_DIR / "prompt-openai.md"
CONFIG_FILE = SCRIPT_DIR / "config.toml"


def screenshot_dir() -> Path:
    """スクショ保存先。XDG_RUNTIME_DIR 配下、未設定なら /tmp に固定で揃える。"""
    base = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
    return Path(base) / "danmaku-loop"


def take_screenshot(dir_: Path, screen: int) -> str:
    """フォルダを空にしてから指定画面を 1 枚撮り、その PNG パスを返す。

    OS ごとに素のコマンドを直接呼ぶ。screen は 0 始まり (danmaku --screen と同じ)。
    """
    shutil.rmtree(dir_, ignore_errors=True)
    dir_.mkdir(parents=True, exist_ok=True)
    timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    out_path = dir_ / f"{timestamp}.png"

    if sys.platform == "darwin":
        # macOS: screencapture。-x はシャッター音抑制、-D は 1 始まりの画面番号。
        # 画面収録権限 (TCC) が無いと黙って真っ黒な画像になる点に注意。
        subprocess.run(
            ["screencapture", "-x", "-D", str(screen + 1), str(out_path)],
            check=True,
        )
    elif sys.platform.startswith("linux"):
        # Linux (X11): xrandr で対象モニタのジオメトリを得て maim -g で切り出す。
        geometry = linux_monitor_geometry(screen)
        subprocess.run(["maim", "-g", geometry, str(out_path)], check=True)
    else:
        raise RuntimeError(f"未対応のプラットフォーム: {sys.platform}")

    return str(out_path)


def linux_monitor_geometry(screen: int) -> str:
    """xrandr の出力から対象モニタのジオメトリ (WxH+X+Y) を返す。

    当面は primary (screen=0) のみ実装。それ以外は明示的に未対応として止める。
    """
    if screen != 0:
        raise RuntimeError(
            f"Linux では screen=0 (primary) のみ対応。指定値: {screen}"
        )
    out = subprocess.run(
        ["xrandr", "--query"], capture_output=True, text=True, check=True
    )
    for line in out.stdout.splitlines():
        if " connected primary " in line:
            for token in line.split():
                # 例: "3840x2160+0+0" の形を探す
                if "x" in token and token.count("+") == 2:
                    return token
            raise RuntimeError(f"primary 行からジオメトリを抽出できない: {line}")
    raise RuntimeError("xrandr 出力に 'connected primary' 行が見つからない")


def encode_image(image_path: str) -> str:
    """PNG を base64 の data URL にして返す (chat/completions の image_url 用)。"""
    data = Path(image_path).read_bytes()
    b64 = base64.b64encode(data).decode("ascii")
    return f"data:image/png;base64,{b64}"


def request_danmaku(image_path: str, prompt_body: str, opts: dict) -> str:
    """画像とプロンプトを /v1/chat/completions に 1 回投げ、本文テキストを返す。

    OpenAI 互換のメッセージ形式 (text + image_url) で送る。返り値はモデルの
    生テキスト (弾幕コメントが改行区切りで並んでいる想定)。HTTP は urllib のみ。
    """
    url = opts["base_url"].rstrip("/") + "/chat/completions"
    payload = {
        "model": opts["model"],
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": prompt_body},
                    {
                        "type": "image_url",
                        "image_url": {"url": encode_image(image_path)},
                    },
                ],
            }
        ],
    }
    body = json.dumps(payload).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    if opts["api_key"]:
        headers["Authorization"] = f"Bearer {opts['api_key']}"

    req = urllib.request.Request(url, data=body, headers=headers, method="POST")
    with urllib.request.urlopen(req) as resp:
        data = json.loads(resp.read().decode("utf-8"))
    return data["choices"][0]["message"]["content"]


def parse_comments(text: str) -> list[str]:
    """モデルの生テキストを改行で分割して弾幕コメントのリストにする。"""
    return text.splitlines()


def send_danmaku(comments: list[str], screen: int) -> None:
    """コメント群を danmaku send で画面に流す。

    danmaku が無い等で失敗しても止めず警告のみ (次ターンへ進めたい)。
    """
    cmd = ["danmaku", "send", "--screen", str(screen), *comments]
    try:
        subprocess.run(cmd, check=False)
    except Exception as e:  # danmaku が無い等
        print(f"danmaku-loop: danmaku send に失敗: {e}", file=sys.stderr)


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
    """API 呼び出しに使う値を解決する。優先順は 引数 > config.toml。"""
    def pick(arg_value, key, default=""):
        if arg_value:
            return arg_value
        return config.get(key) or default

    return {
        "base_url": str(pick(args.base_url, "base_url")),
        "model": str(pick(args.model, "model")),
        "api_key": str(pick(args.api_key, "api_key")),
        "interval": float(pick(args.interval, "interval", 10.0)),
        "count": int(pick(args.count, "count", 1)),
        "screen": int(pick(args.screen, "screen", 0)),
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="スクショを定期取得し OpenAI 互換 API で弾幕を流すループ"
    )
    parser.add_argument(
        "--interval", type=float, default=None,
        help="ターン間隔(秒)。config.toml より優先。既定 10",
    )
    parser.add_argument(
        "--count", type=int, default=None,
        help="実行ターン数。未指定で 1。config.toml より優先",
    )
    parser.add_argument(
        "--screen", type=int, default=None,
        help="撮影/送出する画面番号 (0 始まり)。config.toml より優先。既定 0 (メイン)",
    )
    parser.add_argument(
        "--base-url", default=None,
        help="OpenAI 互換 API の base URL (例 https://api.openai.com/v1)。config.toml より優先",
    )
    parser.add_argument(
        "--model", default=None,
        help="モデル名。config.toml より優先",
    )
    parser.add_argument(
        "--api-key", default=None,
        help="API キー。config.toml より優先",
    )
    args = parser.parse_args()

    prompt_body = PROMPT_FILE.read_text(encoding="utf-8")
    dir_ = screenshot_dir()
    opts = resolve_opts(args, load_config())

    if not opts["base_url"]:
        print("danmaku-loop: base_url が未設定です (--base-url か config.toml)", file=sys.stderr)
        sys.exit(2)
    if not opts["model"]:
        print("danmaku-loop: model が未設定です (--model か config.toml)", file=sys.stderr)
        sys.exit(2)

    print(f"base_url: {opts['base_url']}", flush=True)
    print(f"モデル: {opts['model']}", flush=True)

    turn = 0
    while turn < opts["count"]:
        turn += 1
        print(f"ターン {turn}/{opts['count']} 開始", flush=True)
        try:
            path = take_screenshot(dir_, opts["screen"])
            print(f"  スクショ: {path}", flush=True)
            print("  API 呼び出し", flush=True)
            text = request_danmaku(path, prompt_body, opts)
            comments = parse_comments(text)
            print(f"  弾幕 {len(comments)} 件", flush=True)
            if comments:
                send_danmaku(comments, opts["screen"])
        except urllib.error.HTTPError as e:  # API のエラー応答
            detail = e.read().decode("utf-8", "replace")[:500]
            print(f"danmaku-loop: ターン {turn} API エラー {e.code}: {detail}", file=sys.stderr)
        except Exception as e:  # スクショ失敗・通信失敗等も次ターンへ
            print(f"danmaku-loop: ターン {turn} でエラー: {e}", file=sys.stderr)

        if turn < opts["count"]:
            time.sleep(opts["interval"])


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\ndanmaku-loop: 中断しました", file=sys.stderr)
