use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(name = "danmaku-cli", about = "Send danmaku messages to danmaku-gui via Unix socket")]
struct Args {
    /// 表示先ディスプレイ番号
    #[arg(long, default_value_t = 0)]
    screen: u32,

    /// 文字色（指定時は GUI 側の設定を上書き）
    #[arg(long)]
    color: Option<String>,

    /// 速度倍率（指定時は GUI 側の設定を上書き）
    #[arg(long)]
    speed: Option<f64>,

    /// フォントサイズ（指定時は GUI 側の設定を上書き）
    #[arg(long)]
    size: Option<u32>,

    /// 表示する文字列（複数指定可）
    #[arg(required = true)]
    messages: Vec<String>,
}

#[derive(Serialize)]
struct Payload {
    screen: u32,
    messages: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    speed: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u32>,
}

fn socket_path() -> Result<PathBuf, String> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| "XDG_RUNTIME_DIR is not set; refusing to guess a socket path".to_string())?;
    Ok(PathBuf::from(dir).join("danmaku.sock"))
}

fn run() -> Result<(), String> {
    let args = Args::parse();
    let payload = Payload {
        screen: args.screen,
        messages: args.messages,
        color: args.color,
        speed: args.speed,
        size: args.size,
    };
    let mut line = serde_json::to_string(&payload)
        .map_err(|e| format!("failed to serialize payload: {e}"))?;
    line.push('\n');

    let path = socket_path()?;
    let mut stream = UnixStream::connect(&path)
        .map_err(|e| format!("failed to connect to {}: {e}", path.display()))?;
    stream
        .write_all(line.as_bytes())
        .map_err(|e| format!("failed to write to {}: {e}", path.display()))?;
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("danmaku-cli: {msg}");
            ExitCode::FAILURE
        }
    }
}
