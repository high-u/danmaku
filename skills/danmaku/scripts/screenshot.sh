#!/usr/bin/env bash
# 画面を 1 枚撮り、その PNG の絶対パスを stdout に 1 行で出力する。
# OS 差 (Linux/X11 と macOS) はこのスクリプトで吸収する。SKILL.md 側は
# このスクリプトを呼ぶだけでよい (決定論的な処理をスクリプトに閉じ込める)。
#
# 使い方:
#   screenshot.sh            プライマリ画面 (screen 0) を撮ってパスを出力
#   screenshot.sh <screen>   画面番号 (0 始まり) を指定して撮る
#   screenshot.sh --check     依存コマンドの有無だけ確認し、撮影せず終了
set -euo pipefail

os="$(uname -s)"

# --check: 現在の OS で撮影に必要なコマンドが揃っているかだけ検査する。
if [ "${1:-}" = "--check" ]; then
  case "$os" in
    Darwin)
      command -v screencapture >/dev/null || { echo "screenshot: screencapture が見つかりません" >&2; exit 1; }
      ;;
    Linux)
      command -v maim   >/dev/null || { echo "screenshot: maim が見つかりません (X11 スクショに必要)" >&2; exit 1; }
      command -v xrandr >/dev/null || { echo "screenshot: xrandr が見つかりません (X11 モニタ検出に必要)" >&2; exit 1; }
      ;;
    *)
      echo "screenshot: 未対応のプラットフォーム: $os" >&2; exit 1 ;;
  esac
  exit 0
fi

screen="${1:-0}"

# 保存先: XDG_RUNTIME_DIR > TMPDIR > /tmp の順で揃える。
base="${XDG_RUNTIME_DIR:-${TMPDIR:-/tmp}}"
outdir="${base%/}/danmaku"
mkdir -p "$outdir"
out="$outdir/$(date +%Y%m%d-%H%M%S).png"

case "$os" in
  Darwin)
    # screencapture: -x はシャッター音抑制、-D は 1 始まりの画面番号。
    # 画面収録権限 (TCC) が無いと黙って真っ黒な画像になる点に注意。
    screencapture -x -D "$((screen + 1))" "$out"
    ;;
  Linux)
    # X11: xrandr で対象モニタのジオメトリ (WxH+X+Y) を得て maim -g で切り出す。
    # 当面はプライマリ (screen=0) のみ。それ以外は明示的に未対応として止める。
    if [ "$screen" -ne 0 ]; then
      echo "screenshot: Linux では screen=0 (primary) のみ対応。指定値: $screen" >&2
      exit 1
    fi
    geometry="$(xrandr --query | awk '/ connected primary / {
      for (i = 1; i <= NF; i++) if ($i ~ /^[0-9]+x[0-9]+\+[0-9]+\+[0-9]+$/) { print $i; exit }
    }')"
    if [ -z "$geometry" ]; then
      echo "screenshot: xrandr 出力から primary のジオメトリを取得できません" >&2
      exit 1
    fi
    maim -g "$geometry" "$out"
    ;;
  *)
    echo "screenshot: 未対応のプラットフォーム: $os" >&2
    exit 1
    ;;
esac

echo "$out"
