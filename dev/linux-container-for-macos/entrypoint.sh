#!/usr/bin/env bash
# 起動順は意味がある:
#   Xvnc (実Xディスプレイ + VNC公開) → XFCE セッション (startxfce4) → 待機
# XFCE は壁紙・パネル・端末を持つ実デスクトップ。透過合成は xfwm4 内蔵コンポジタが担う。
set -euo pipefail

DISPLAY_NUM="${DISPLAY#:}"   # ":1" -> "1"
GEOMETRY="${VNC_GEOMETRY:-1600x900}"
DEPTH="${VNC_DEPTH:-24}"
VNC_PASSWORD="${VNC_PASSWORD:-danmaku}"
PASSWD_FILE="/tmp/.vncpasswd"

# VncAuth 用のパスワードファイルを生成。
# macOS の「画面共有」は SecurityTypes None を扱えずパスワードを要求するため、
# 認証なしではなく VncAuth (パスワード認証) にする。vncpasswd -f は平文を受けて
# 難読化済みパスワードを stdout に出す (tigervnc-common 同梱)。
printf '%s\n' "${VNC_PASSWORD}" | vncpasswd -f > "${PASSWD_FILE}"
chmod 600 "${PASSWD_FILE}"

cleanup() {
    # コンテナ停止時に X のロックを残さない。
    pkill -x Xvnc 2>/dev/null || true
    pkill -f xfce 2>/dev/null || true
    rm -f "/tmp/.X${DISPLAY_NUM}-lock" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# VncAuth (パスワード認証)。Mac の「画面共有」からパスワード "danmaku" で接続する。
Xvnc "${DISPLAY}" \
    -geometry "${GEOMETRY}" -depth "${DEPTH}" \
    -rfbport 5901 \
    -SecurityTypes VncAuth \
    -PasswordFile "${PASSWD_FILE}" \
    -AlwaysShared \
    -desktop danmaku-dev &
XVNC_PID=$!

# Xvnc がソケットを開くまで待つ。
for _ in $(seq 1 50); do
    if xdpyinfo -display "${DISPLAY}" >/dev/null 2>&1; then break; fi
    sleep 0.1
done

# XFCE セッションを起動 (dbus セッションバス込み)。
# xfwm4 の内蔵コンポジタを有効化して透過/クリックスルーを合成させる。
# startxfce4 は session bus が要るので dbus-launch 経由で起動する。
dbus-launch --exit-with-session startxfce4 >/tmp/xfce.log 2>&1 &

# xfwm4 が立ち上がってから合成を有効化 (xfconf-query はセッション起動後でないと効かない)。
( for _ in $(seq 1 50); do
      if pgrep -x xfwm4 >/dev/null 2>&1; then
          xfconf-query -c xfwm4 -p /general/use_compositing -s true 2>/dev/null || true
          break
      fi
      sleep 0.2
  done ) &

echo "danmaku-dev: VNC ready on :5901 (DISPLAY=${DISPLAY}, ${GEOMETRY}x${DEPTH})"
echo "  Mac の「画面共有」で  vnc://localhost:5901  へ接続 (パスワード: ${VNC_PASSWORD})。"

# Xvnc が生きている限りコンテナを保つ (短命な補助ジョブで終了しないよう PID 名指し)。
wait "${XVNC_PID}"
