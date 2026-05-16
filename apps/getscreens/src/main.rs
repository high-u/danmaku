use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use chrono::Local;
use clap::Parser;
use image::imageops::FilterType;
use serde::Serialize;

#[derive(Parser, Debug)]
#[command(
    name = "getscreens",
    about = "メインモニターをキャプチャして JSON 配列でパスを返す",
    long_about = "Linux (X11) では maim + xrandr を内部で呼び出して PNG を保存し、stdout に JSON 配列を 1 行出力する。`--size` が指定された場合は長辺をその値にリサイズする。"
)]
struct Cli {
    /// 保存先ディレクトリ。<dir>/<screen>/<timestamp>.png に保存する。
    /// 未指定の場合は $XDG_RUNTIME_DIR/getscreens、未設定なら /tmp/getscreens
    #[arg(long)]
    dir: Option<PathBuf>,

    /// リサイズ後の長辺ピクセル。未指定なら縮小なし
    #[arg(long)]
    size: Option<u32>,
}

fn default_dir() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(v) if !v.is_empty() => PathBuf::from(v).join("getscreens"),
        _ => PathBuf::from("/tmp/getscreens"),
    }
}

#[derive(Serialize)]
struct ScreenEntry {
    screen: u32,
    path: String,
    timestamp: String,
}

#[derive(Debug)]
struct Monitor {
    index: u32,
    geometry: String,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("getscreens: {err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    ensure_command_available("maim")?;
    ensure_command_available("xrandr")?;

    let monitor = primary_monitor().context("プライマリモニターの特定に失敗")?;
    let timestamp = Local::now().format("%Y%m%d-%H%M%S").to_string();

    let base_dir = cli.dir.unwrap_or_else(default_dir);
    let screen_dir = base_dir.join(monitor.index.to_string());
    std::fs::create_dir_all(&screen_dir)
        .with_context(|| format!("保存先ディレクトリの作成に失敗: {}", screen_dir.display()))?;
    let out_path = screen_dir.join(format!("{timestamp}.png"));

    capture_with_maim(&monitor.geometry, &out_path)?;
    if let Some(size) = cli.size {
        resize_in_place(&out_path, size)?;
    }

    let entries = vec![ScreenEntry {
        screen: monitor.index,
        path: out_path
            .to_str()
            .context("出力パスが UTF-8 でない")?
            .to_string(),
        timestamp,
    }];
    println!("{}", serde_json::to_string(&entries)?);
    Ok(())
}

fn ensure_command_available(name: &str) -> Result<()> {
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {name}"))
        .output()
        .with_context(|| format!("`command -v {name}` の実行に失敗"))?;
    if !output.status.success() {
        bail!("`{name}` が見つかりません。インストールしてください");
    }
    Ok(())
}

fn primary_monitor() -> Result<Monitor> {
    let output = Command::new("xrandr")
        .arg("--query")
        .output()
        .context("`xrandr --query` の実行に失敗")?;
    if !output.status.success() {
        bail!(
            "xrandr が非ゼロで終了: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8(output.stdout).context("xrandr 出力が UTF-8 でない")?;

    for line in stdout.lines() {
        if line.contains(" connected primary ") {
            // 例: "HDMI-A-0 connected primary 3840x2160+0+0 (...)"
            for token in line.split_whitespace() {
                if is_geometry(token) {
                    return Ok(Monitor {
                        index: 0,
                        geometry: token.to_string(),
                    });
                }
            }
            bail!("primary 行からジオメトリを抽出できなかった: {line}");
        }
    }
    bail!("xrandr 出力に 'connected primary' 行が見つからない");
}

fn is_geometry(token: &str) -> bool {
    let has_x = token.contains('x');
    let has_plus = token.matches('+').count() == 2;
    has_x && has_plus && token.chars().all(|c| c.is_ascii_digit() || c == 'x' || c == '+')
}

fn capture_with_maim(geometry: &str, out_path: &Path) -> Result<()> {
    let status = Command::new("maim")
        .arg("-g")
        .arg(geometry)
        .arg(out_path)
        .status()
        .context("maim の実行に失敗")?;
    if !status.success() {
        bail!("maim が非ゼロで終了 (geometry={geometry})");
    }
    Ok(())
}

fn resize_in_place(path: &Path, max_side: u32) -> Result<()> {
    let img = image::open(path)
        .with_context(|| format!("画像の読み込みに失敗: {}", path.display()))?;
    let (w, h) = (img.width(), img.height());
    if w <= max_side && h <= max_side {
        return Ok(());
    }
    let resized = img.resize(max_side, max_side, FilterType::Triangle);
    resized
        .save(path)
        .with_context(|| format!("リサイズ後の保存に失敗: {}", path.display()))?;
    Ok(())
}
