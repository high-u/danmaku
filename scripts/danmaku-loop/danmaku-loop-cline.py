#!/usr/bin/env python3
"""画面のスクショを定期的に撮り、cline CLI に渡して弾幕を流すループ。

各ターンの流れ:
  1. スクショ保存先フォルダを空にする
  2. OS ごとの素のコマンドでスクショを撮り、その PNG パスを得る
  3. cline を 1 回だけ呼ぶ (プロンプト先頭に @<絶対パス> を付けて画像を添付)
     cline はこのメンションを画像と判定し base64 化してモデルへ送る。
     コメント生成と danmaku send の実行は cline 側が行う (--act --yolo)
  4. interval 秒待つ

cline の終了コードや出力は見ない。あるターンで失敗しても淡々と次のターンへ進む。
プロンプト本文は同じフォルダの prompt.md を編集すれば差し替えられる。

OpenAI 互換 API の指定は次の優先順で解決する (上が強い):
  1. CLI 引数 (--base-url / --model / --api-key)
  2. 同階層の config.toml (あれば自動で読む)
cline は base_url を実行毎フラグで受け取れないため、起動時に一度だけ
`cline auth` で専用の設定ディレクトリ (CLINE_CONFIG_DIR) に書き込む。
このディレクトリは danmaku-loop 専用で、ユーザ既定の cline 設定には触れない。
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


def screenshot_dir() -> Path:
    """スクショ保存先。XDG_RUNTIME_DIR 配下、未設定なら /tmp に固定で揃える。"""
    base = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
    return Path(base) / "danmaku-loop"


def cline_work_dir() -> Path:
    """cline を実行する作業ディレクトリ (cwd)。

    cline はコーディングエージェントであり cwd にファイルを書き出すことがある。
    スクショ用ディレクトリは毎ターン削除するので別に用意し、万一の書き込みを
    リポジトリ外のこの捨てフォルダに閉じ込める。
    """
    base = os.environ.get("XDG_RUNTIME_DIR") or "/tmp"
    return Path(base) / "danmaku-loop-work"


def cline_config_dir() -> Path:
    """cline の設定 (プロバイダ/モデル/APIキー) を置く danmaku-loop 専用ディレクトリ。

    ユーザ既定の cline 設定 (~/.cline 等) を汚さないよう、ここに隔離する。
    XDG_CONFIG_HOME 配下、未設定なら ~/.config を使う。
    """
    base = os.environ.get("XDG_CONFIG_HOME") or str(Path.home() / ".config")
    return Path(base) / "danmaku-loop" / "cline"


def take_screenshot(dir_: Path, screen: int) -> str:
    """フォルダを空にしてから指定画面を 1 枚撮り、その PNG パスを返す。

    かつては自作の getscreens (Rust) を介していたが、簡易スクリプトに
    Rust ラッパーを挟むのは過剰なため、OS ごとに素のコマンドを直接呼ぶ。
    OS 差の吸収という抽象は SKILL.md 側 (getscreens) が担い、ここは直叩きでよい。
    screen は 0 始まり (danmaku --screen と同じ番号体系)。今はメイン相当のみ運用。
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
        # Linux (X11): getscreens 相当を直接実行。xrandr で対象モニタの
        # ジオメトリを得て maim -g で切り出す。
        geometry = linux_monitor_geometry(screen)
        subprocess.run(["maim", "-g", geometry, str(out_path)], check=True)
    else:
        raise RuntimeError(f"未対応のプラットフォーム: {sys.platform}")

    return str(out_path)


def linux_monitor_geometry(screen: int) -> str:
    """xrandr の出力から対象モニタのジオメタリ (WxH+X+Y) を返す。

    マルチモニタ対応の足場として screen を受け取るが、当面は primary (screen=0)
    のみ実装。それ以外が来たら明示的に未対応として止める。
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


def configure_cline(opts: dict, config_dir: Path) -> None:
    """cline の OpenAI 互換プロバイダを専用設定ディレクトリへ書き込む (起動時 1 回)。

    cline は base_url を実行毎フラグで受け取れないため、先に `cline auth` で
    プロバイダ/モデル/APIキーを保存しておく。config.toml を単一ソースに保つため
    毎起動でここを上書きする (冪等)。失敗しても止めず警告のみ (各ターンで顕在化)。
    """
    config_dir.mkdir(parents=True, exist_ok=True)
    cmd = [
        "cline", "auth",
        "--config", str(config_dir),
        "--provider", "openai",
        "--apikey", opts["api_key"],
        "--baseurl", opts["base_url"],
        "--modelid", opts["model"],
    ]
    try:
        subprocess.run(cmd, check=False)
    except Exception as e:  # cline が無い等。ループ自体は回したいので止めない
        print(f"danmaku-loop: cline auth に失敗: {e}", file=sys.stderr)


def run_cline(
    image_path: str, prompt_body: str, opts: dict, config_dir: Path, work_dir: Path
) -> None:
    """画像を添付してプロンプトを 1 回投げる。

    `@<絶対パス>` のメンションで画像を添付する (cline が base64 化して送る)。
    --act --yolo でツール実行 (danmaku send) まで自動承認する。cwd は捨てフォルダ。
    cline の stdout/stderr はそのまま端末に出る (隠さない)。終了コードは見ない。
    """
    work_dir.mkdir(parents=True, exist_ok=True)
    prompt = f"@{image_path}\n\n{prompt_body}"
    cmd = [
        "cline",
        "--config", str(config_dir),
        "--cwd", str(work_dir),
        "--act", "--yolo",
        "-m", opts["model"],
        prompt,
    ]
    try:
        subprocess.run(cmd, check=False)
    except Exception as e:  # cline が無い等。次ターンへ進めるため止めない
        print(f"danmaku-loop: cline 呼び出しに失敗: {e}", file=sys.stderr)


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
    """cline に渡す OpenAI 互換の値を解決する。
    優先順は 引数 > config.toml。どちらにも無ければ空文字。
    環境変数は設定ソースにしない (cline 既定の設定には介入しない)。
    """
    def pick(arg_value, key):
        if arg_value:
            return arg_value
        return str(config.get(key) or "")

    def pick_num(arg_value, key, default):
        if arg_value is not None:
            return arg_value
        return config.get(key, default)

    return {
        "base_url": pick(args.base_url, "base_url"),
        "model": pick(args.model, "model"),
        "api_key": pick(args.api_key, "api_key"),
        "interval": float(pick_num(args.interval, "interval", 10.0)),
        "count": int(pick_num(args.count, "count", 1)),
        "screen": int(pick_num(args.screen, "screen", 0)),
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="スクショを定期取得し cline CLI に弾幕を流させるループ"
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
        "--base-url", default=None,
        help="cline の --baseurl に渡す値。config.toml より優先",
    )
    parser.add_argument(
        "--model", default=None,
        help="cline の -m / --modelid に渡すモデル名。config.toml より優先",
    )
    parser.add_argument(
        "--api-key", default=None,
        help="cline の --apikey に渡す値。config.toml より優先",
    )
    args = parser.parse_args()

    prompt_body = PROMPT_FILE.read_text(encoding="utf-8")
    dir_ = screenshot_dir()
    config_dir = cline_config_dir()
    work_dir = cline_work_dir()
    opts = resolve_opts(args, load_config())

    # 起動時に一度だけ cline のプロバイダ設定を同期する。
    print(f"モデル: {opts['model'] or '(未指定)'}", flush=True)
    configure_cline(opts, config_dir)

    turn = 0
    while turn < opts["count"]:
        turn += 1
        print(f"ターン {turn}/{opts['count']} 開始", flush=True)
        try:
            path = take_screenshot(dir_, opts["screen"])
            print(f"  スクショ: {path}", flush=True)
            print("  cline 呼び出し", flush=True)
            run_cline(path, prompt_body, opts, config_dir, work_dir)
        except Exception as e:  # スクショ失敗等も次ターンへ
            print(f"danmaku-loop: ターン {turn} でエラー: {e}", file=sys.stderr)

        if turn < opts["count"]:
            time.sleep(opts["interval"])


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        print("\ndanmaku-loop: 中断しました", file=sys.stderr)
