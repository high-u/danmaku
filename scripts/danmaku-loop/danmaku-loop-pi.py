#!/usr/bin/env python3
"""画面のスクショを定期的に撮り、pi CLI に渡して弾幕を流すループ。

cline 版 (danmaku-loop-cline.py) と同じ構成の pi 版。エージェント (pi) が
画像を見てコメントを作り、bash ツールで `danmaku send` を実行する。

各ターンの流れ:
  1. スクショ保存先フォルダを空にする
  2. OS ごとの素のコマンドでスクショを撮り、その PNG パスを得る
  3. pi を 1 回だけ呼ぶ (画像は `@<絶対パス>` を独立トークンで渡す)
     pi はこの画像を読み、コメント生成と danmaku send の実行を行う (-p で 1 ショット)
  4. interval 秒待つ

pi の終了コードや出力は見ない。あるターンで失敗しても淡々と次のターンへ進む。
プロンプト本文は同じフォルダの prompt.md を編集すれば差し替えられる。

接続先 (OpenAI 互換 API の base URL / API キー) は cline 版と違い実行毎フラグでは
渡せない。pi は同階層 pi-agent/models.json でプロバイダを定義する設計なので、
このスクリプトは PI_CODING_AGENT_DIR をその pi-agent に向けて pi に読ませる。
models.json は LM Studio など接続先に合わせて事前に書いておく (起動時生成はしない)。

config.toml では次を解決する (上が強い):
  1. CLI 引数 (--provider / --model / --interval / --count / --screen)
  2. 同階層の config.toml (あれば自動で読む)
base_url / api_key は models.json 側が持つため、ここでは扱わない。
"""

import argparse
import os
import shutil
import subprocess
import sys
import time
import tomllib
from datetime import datetime
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
PROMPT_FILE = SCRIPT_DIR / "prompt.md"
CONFIG_FILE = SCRIPT_DIR / "config.toml"
# pi はこのディレクトリ直下の models.json でプロバイダ (接続先) を読む。
PI_AGENT_DIR = SCRIPT_DIR / "pi-agent"


def screenshot_dir() -> Path:
    """スクショ保存先。XDG_RUNTIME_DIR 配下、未設定なら /tmp に固定で揃える。"""
    base = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
    return Path(base) / "danmaku-loop"


def pi_work_dir() -> Path:
    """pi を実行する作業ディレクトリ (cwd)。

    pi はコーディングエージェントであり cwd にファイルを書き出すことがある。
    スクショ用ディレクトリは毎ターン削除するので別に用意し、万一の書き込みを
    リポジトリ外のこの捨てフォルダに閉じ込める。
    """
    base = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
    return Path(base) / "danmaku-loop-work"


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


def run_pi(
    image_path: str, prompt_body: str, opts: dict, work_dir: Path
) -> None:
    """画像を添付してプロンプトを 1 回投げる。

    `@<絶対パス>` を独立トークンで渡して画像を添付する (cline と違い本文には埋めない)。
    -p で 1 ショット (応答後 exit)。ツールは bash のみ許可し danmaku send を実行させる。
    --no-session でセッションを残さず、cwd は捨てフォルダ。接続先は PI_CODING_AGENT_DIR
    に置いた models.json から読む。pi の stdout/stderr はそのまま出す。終了コードは見ない。
    """
    work_dir.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env["PI_CODING_AGENT_DIR"] = str(PI_AGENT_DIR)
    cmd = [
        "pi", "-p",
        "--no-session",
        "--no-extensions", "--no-skills", "--no-prompt-templates",
        "--tools", "bash",
        "--provider", opts["provider"],
        "--model", opts["model"],
        f"@{image_path}",
        prompt_body,
    ]
    try:
        subprocess.run(cmd, check=False, cwd=str(work_dir), env=env)
    except Exception as e:  # pi が無い等。次ターンへ進めるため止めない
        print(f"danmaku-loop: pi 呼び出しに失敗: {e}", file=sys.stderr)


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
    """pi に渡す値を解決する。優先順は 引数 > config.toml。
    base_url / api_key は models.json が持つためここでは扱わない。
    """
    def pick(arg_value, key, default=""):
        if arg_value:
            return arg_value
        return str(config.get(key) or default)

    def pick_num(arg_value, key, default):
        if arg_value is not None:
            return arg_value
        return config.get(key, default)

    return {
        "provider": pick(args.provider, "provider", "lmstudio"),
        "model": pick(args.model, "model"),
        "interval": float(pick_num(args.interval, "interval", 10.0)),
        "count": int(pick_num(args.count, "count", 1)),
        "screen": int(pick_num(args.screen, "screen", 0)),
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="スクショを定期取得し pi CLI に弾幕を流させるループ"
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
        help="撮影する画面番号 (0 始まり)。config.toml より優先。既定 0 (メイン)",
    )
    parser.add_argument(
        "--provider", default=None,
        help="pi の --provider に渡す名前 (models.json のプロバイダ名)。config.toml より優先。既定 lmstudio",
    )
    parser.add_argument(
        "--model", default=None,
        help="pi の --model に渡すモデル ID (models.json の id と一致させる)。config.toml より優先",
    )
    args = parser.parse_args()

    prompt_body = PROMPT_FILE.read_text(encoding="utf-8")
    dir_ = screenshot_dir()
    work_dir = pi_work_dir()
    opts = resolve_opts(args, load_config())

    if not opts["model"]:
        print("danmaku-loop: model が未設定です (--model か config.toml)", file=sys.stderr)
        sys.exit(2)
    if not (PI_AGENT_DIR / "models.json").exists():
        print(f"danmaku-loop: {PI_AGENT_DIR / 'models.json'} が見つかりません", file=sys.stderr)
        sys.exit(2)

    print(f"プロバイダ: {opts['provider']}", flush=True)
    print(f"モデル: {opts['model']}", flush=True)

    turn = 0
    while turn < opts["count"]:
        turn += 1
        print(f"ターン {turn}/{opts['count']} 開始", flush=True)
        try:
            path = take_screenshot(dir_, opts["screen"])
            print(f"  スクショ: {path}", flush=True)
            print("  pi 呼び出し", flush=True)
            run_pi(path, prompt_body, opts, work_dir)
        except Exception as e:  # スクショ失敗等も次ターンへ
            print(f"danmaku-loop: ターン {turn} でエラー: {e}", file=sys.stderr)

        if turn < opts["count"]:
            time.sleep(opts["interval"])


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\ndanmaku-loop: 中断しました", file=sys.stderr)
