//! procon-light: Pro Controller のようなゲームパッドで、dmxctl の照明を動かす。
//! コントローラーの操作を OSC にして dmxctl に送る(USB-DMX・Art-Net への出力とレーザーの安全の処理は dmxctl が受け持つ)。
//! 起動するとウィンドウを出す(gui.rs)。`--cli` なら端末で動かす。操作の割り当ては src/control.rs の先頭の表

// Windows のリリース版は、ダブルクリックで起動したときに黒い端末の窓を出さない
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
// SDL3 に内蔵の hidapi と hidapi クレートが、Mac だけの関数を2つずつ持っていて、リンクで「重なっている」と出る
// (hid_darwin_get_location_id と hid_darwin_is_device_open_exclusive)。SDL3 もこのツールもこの2つを呼ばないので、
// どちらが使われても動きは変わらない
#![allow(linker_messages)]

mod control;
mod demo;
mod engine;
mod fonts;
mod gui;
mod osc;
mod padmap;
mod rig;
mod shot;
mod switch2;

use engine::{Options, lock};
use std::io::Write;
use std::sync::atomic::Ordering;

const USAGE: &str = "procon-light [--to HOST:PORT] [--movers 1,3,4] [--test] [--demo] [--cli]
  --to HOST:PORT   dmxctl の OSC 受信の送り先(既定: 前に画面で決めたもの。無ければ 127.0.0.1 と dmxctl の設定の OSC ポート)
  --movers ID,...  左スティックで動かすムービングの番号(既定: dmxctl の設定から、パン・チルトのある機材。レーザーは除く)
  --test           送らずに、コントローラーの入力だけを見る
  --demo           コントローラーを使わず、決まった操作(色を回す・ムービングを動かす・フラッシュなど)を送り続ける
  --cli            ウィンドウを出さず、端末で動かす(Ctrl+C で終わる)
";

fn parse_args() -> Result<(Options, bool), String> {
    let mut o = Options::default();
    let mut cli = false;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--to" => o.to = Some(args.next().ok_or("--to には送り先が要ります")?),
            "--movers" => {
                let v = args.next().ok_or("--movers には番号が要ります")?;
                let ids: Result<Vec<u32>, _> = v.split(',').filter(|s| !s.is_empty()).map(|s| s.trim().parse()).collect();
                o.movers = Some(ids.map_err(|_| format!("--movers の番号が読めません: {v}"))?);
            }
            "--test" => o.test = true,
            "--demo" => o.demo = true,
            "--cli" => cli = true,
            "-h" | "--help" | "help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            // Mac で .app を Finder から開いたときに付くことがある
            s if s.starts_with("-psn_") => {}
            _ => return Err(format!("知らないオプションです: {a}\n{USAGE}")),
        }
    }
    Ok((o, cli))
}

fn main() {
    let (opts, cli) = parse_args().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    let result = if cli { run_cli(opts) } else { gui::run(opts) };
    if let Err(e) = result {
        eprintln!("\n{e}");
        std::process::exit(1);
    }
}

/// `--cli`: 端末の1行に様子を出しながら動かす
fn run_cli(opts: Options) -> Result<(), String> {
    let (ctl, shared) = engine::setup(&opts, gui::saved_to());
    let shared = std::sync::Arc::new(shared);
    {
        let shared = shared.clone();
        let _ = ctrlc::set_handler(move || shared.stop.store(true, Ordering::SeqCst));
    }
    {
        let s = lock(&shared.status);
        let to = lock(&shared.settings).to.clone();
        println!("procon-light: 送り先 {to}{}", if opts.test { "(テスト: 送らない)" } else { "" });
        if !s.notice.is_empty() {
            println!("{}", s.notice);
        }
        if s.osc_off && opts.to.is_none() {
            println!("注意: dmxctl の OSC 受信が OFF です。dmxctl の設定で OSC 受信を ON にしてください");
        }
        println!("ムービング {}台 / LED バーのエフェクト {}個", s.movers.len(), s.pixel_fx);
    }
    let draw_shared = shared.clone();
    engine::run(&shared, ctl, &move || draw(&draw_shared))?;
    println!("\n終わりました");
    Ok(())
}

/// 端末の1行に、今の様子を出す(色は実際の色で塗る)
fn draw(shared: &engine::Shared) {
    let s = lock(&shared.status).clone();
    let test = lock(&shared.settings).test;
    let v = &s.view;
    let (r, g, b) = v.rgb;
    let swatch = if v.color_on { format!("\x1b[48;2;{r};{g};{b}m      \x1b[0m") } else { "(パレット)".into() };
    let state = if s.pad.is_none() && !s.demo {
        "未接続".to_string()
    } else if v.paused {
        "一時停止".to_string()
    } else {
        format!(
            "色 {swatch} {:>3}° {:>3}%{} | 明るさ {:>3}%{} | 向き {:>3}% {:>3}%{}",
            (v.hue * 360.0).round() as i32,
            (v.sat * 100.0).round() as i32,
            if v.auto_hue { " 回転中" } else { "" },
            (v.master * 100.0).round() as i32,
            if v.faded { " 暗転" } else { "" },
            (v.pan * 100.0).round() as i32,
            (v.tilt * 100.0).round() as i32,
            if v.gyro_aim { " ジャイロ" } else { "" },
        )
    };
    let notice = if test && !s.held.is_empty() { format!("押している: {:?}", s.held) } else { s.notice.clone() };
    print!("\r\x1b[2K{state} | {notice}");
    let _ = std::io::stdout().flush();
}
