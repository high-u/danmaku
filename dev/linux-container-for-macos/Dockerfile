# danmaku-linux-x11 開発用イメージ。
# 目的: macOS から「実 X11 ディスプレイ + 合成器」を VNC 越しに触り、
#       透過オーバーレイ / クリックスルーの見た目まで確認できる環境を用意する。
# ソースはイメージに焼かず compose 側でマウントする (ビルドはコンテナ内)。
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# 1) GTK4 / cairo / pango など danmaku-linux-x11 のビルドに要る -dev 群
# 2) X サーバ込みの TigerVNC (Xvnc) と、実デスクトップ環境 XFCE
#    (xfwm4 の内蔵コンポジタで透過を賄うため picom は不要)
# 3) Rust ツールチェインのビルドに要る最小限 (rustup は後段で導入)
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl pkg-config build-essential \
        libgtk-4-dev libgdk-pixbuf-2.0-dev libcairo2-dev libpango1.0-dev \
        libglib2.0-dev libx11-dev \
        tigervnc-standalone-server tigervnc-common tigervnc-tools \
        xfce4 xfce4-terminal \
        x11-xserver-utils x11-utils \
        dbus-x11 \
    && rm -rf /var/lib/apt/lists/*

# Rust (rustup)。CI ではなく対話開発用なので stable を入れる。
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
ENV PATH=/root/.cargo/bin:$PATH

# Xvnc が使う実ディスプレイ番号と、アプリ/cargo が見る DISPLAY を固定。
ENV DISPLAY=:1

# コンテナにアクセシビリティバスは無い。GTK の a11y 警告を抑止 (GTK 自身の推奨)。
ENV GTK_A11Y=none

# UnixSocket (danmaku-<screen>.sock) の置き場所。アプリは XDG_RUNTIME_DIR 必須。
ENV XDG_RUNTIME_DIR=/tmp/runtime-root
RUN mkdir -p /tmp/runtime-root && chmod 700 /tmp/runtime-root

WORKDIR /workspace

COPY entrypoint.sh /usr/local/bin/entrypoint.sh
RUN chmod +x /usr/local/bin/entrypoint.sh

EXPOSE 5901
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
