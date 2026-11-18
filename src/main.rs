//! procon-light: Pro Controller のようなゲームパッドで、dmxctl の照明を動かす。
//! コントローラーの操作を OSC にして dmxctl に送る(USB-DMX・Art-Net への出力とレーザーの安全の処理は dmxctl が受け持つ)。
//! 操作の割り当ては src/control.rs の先頭の表

// SDL3 に内蔵の hidapi と hidapi クレートが、Mac だけの関数を2つずつ持っていて、リンクで「重なっている」と出る
// (hid_darwin_get_location_id と hid_darwin_is_device_open_exclusive)。SDL3 もこのツールもこの2つを呼ばないので、
// どちらが使われても動きは変わらない
#![allow(linker_messages)]

mod control;
mod demo;
mod osc;
mod rig;
mod switch2;

use control::{Btn, Controller, Input};
use sdl3::event::Event;
use sdl3::gamepad::{Axis, Button, ButtonLabel, Gamepad};
use sdl3::sensor::SensorType;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const USAGE: &str = "procon-light [--to HOST:PORT] [--movers 1,3,4] [--test] [--demo]
  --to HOST:PORT   dmxctl の OSC 受信の送り先(既定: 127.0.0.1 と dmxctl の設定の OSC ポート)
  --movers ID,...  左スティックで動かすムービングの番号(既定: dmxctl の設定から、パン・チルトのある機材。レーザーは除く)
  --test           送らずに、コントローラーの入力だけを見る
  --demo           コントローラーを使わず、決まった操作(色を回す・ムービングを動かす・フラッシュなど)を送り続ける
";

struct Options {
    to: Option<String>,
    movers: Option<Vec<u32>>,
    test: bool,
    demo: bool,
}

fn parse_args() -> Result<Options, String> {
    let mut o = Options { to: None, movers: None, test: false, demo: false };
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
            "-h" | "--help" | "help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            _ => return Err(format!("知らないオプションです: {a}\n{USAGE}")),
        }
    }
    Ok(o)
}

fn main() {
    let opts = parse_args().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    if let Err(e) = run(opts) {
        eprintln!("\n{e}");
        std::process::exit(1);
    }
}

fn run(opts: Options) -> Result<(), String> {
    // dmxctl の設定から、ムービングと LED バーのエフェクト、OSC のポートを読む
    let dir = rig::config_dir();
    let project: Option<serde_json::Value> = std::fs::read_to_string(dir.join("project.json")).ok().and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok());
    let found = match rig::Rig::load(&dir) {
        Ok(r) => r,
        Err(e) => {
            println!("dmxctl の設定を読めませんでした({e})。ムービングと LED バーのエフェクトは使えません");
            rig::Rig::default()
        }
    };
    let osc_port = project.as_ref().and_then(|p| p["osc_port"].as_u64()).unwrap_or(9000);
    if project.as_ref().is_some_and(|p| p["osc_enabled"] == false) && opts.to.is_none() {
        println!("注意: dmxctl の OSC 受信が OFF です。dmxctl の設定で OSC 受信を ON にしてください(ポート {osc_port})");
    }
    let movers = opts.movers.clone().unwrap_or(found.movers);
    let to = opts.to.clone().unwrap_or_else(|| format!("127.0.0.1:{osc_port}"));
    let sender = osc::Sender::new(&to).map_err(|e| e.to_string())?;

    if opts.demo {
        return run_demo(&sender, Controller::new(movers, found.pixel_fx, 1));
    }

    // SDL の設定は、初期化の前に決める
    // 端末から動かすのでウィンドウが無い。手前のアプリでなくても入力を受ける
    sdl3::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
    sdl3::hint::set("SDL_JOYSTICK_HIDAPI", "1");
    sdl3::hint::set("SDL_JOYSTICK_HIDAPI_SWITCH", "1");
    // Switch 2 Pro Controller は src/switch2.rs で読む。SDL には触らせない
    sdl3::hint::set("SDL_JOYSTICK_HIDAPI_SWITCH2", "0");
    sdl3::hint::set("SDL_HIDAPI_LIBUSB", "0");
    let sdl = sdl3::init().map_err(|e| e.to_string())?;
    let gc = sdl.gamepad().map_err(|e| e.to_string())?;
    let mut events = sdl.event_pump().map_err(|e| e.to_string())?;
    let mut hid = hidapi::HidApi::new().map_err(|e| format!("HID を使えません: {e}"))?;

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        let _ = ctrlc::set_handler(move || stop.store(true, Ordering::SeqCst));
    }

    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    let mut ctl = Controller::new(movers.clone(), found.pixel_fx.clone(), seed);
    println!("procon-light: 送り先 {}{}", sender.to(), if opts.test { "(テスト: 送らない)" } else { "" });
    println!(
        "ムービング {}台{} / LED バーのエフェクト {}個",
        movers.len(),
        if movers.is_empty() { String::new() } else { format!("(番号 {})", movers.iter().map(u32::to_string).collect::<Vec<_>>().join(",")) },
        found.pixel_fx.len()
    );
    println!("右スティック=色  左スティック=向き  ZR/ZL=フラッシュ  R=タップ  L=頭出し  A=入れ替え  B=暗転  X=ランダム色  Y=色相を回す");
    println!("十字上下=明るさ  十字左右=LEDエフェクト  −=パレットの色に戻す  +=一時停止  キャプチャ=ジャイロで狙う  Ctrl+C=終わる");

    let mut pad: Option<Pad> = None;
    let mut looked_for_switch2 = Instant::now() - Duration::from_secs(10);
    let mut gyro: Option<(f32, f32)> = None;
    let mut last = Instant::now();
    let mut drawn = Instant::now() - Duration::from_secs(1);
    let mut notice = "コントローラーを待っています(有線でつなぐか、Bluetooth でペアリングしてください)".to_string();

    while !stop.load(Ordering::SeqCst) {
        for ev in events.poll_iter() {
            match ev {
                Event::Quit { .. } => stop.store(true, Ordering::SeqCst),
                Event::GamepadAdded { which, .. } if pad.is_none() => match gc.open(which) {
                    // Switch 2 Pro Controller は SDL では読めないので、下で別に開く
                    Ok(c) if c.vendor_id() == Some(switch2::VENDOR) && c.product_id() == Some(switch2::PRODUCT) => {}
                    Ok(mut c) => {
                        // ジャイロが無いコントローラーもある。そのときはジャイロで狙えないだけ
                        // SAFETY: 開いたばかりの有効なゲームパッドに聞くだけ
                        let has_gyro = unsafe { c.has_sensor(SensorType::Gyroscope) };
                        if has_gyro {
                            let _ = c.sensor_set_enabled(SensorType::Gyroscope, true);
                        }
                        let _ = c.set_player_index(0);
                        let _ = c.set_rumble(0x4000, 0x4000, 150);
                        let labels = Labels::of(&c);
                        notice = format!("つながりました: {}{}", c.name().unwrap_or_default(), if has_gyro { "" } else { "(ジャイロなし)" });
                        pad = Some(Pad::Sdl(c, labels));
                    }
                    Err(e) => notice = format!("コントローラーを開けません: {e}"),
                },
                Event::GamepadRemoved { which, .. } if matches!(&pad, Some(Pad::Sdl(c, _)) if c.id().ok() == Some(which)) => {
                    disconnected(&mut ctl, &sender, opts.test);
                    pad = None;
                    gyro = None;
                    notice = "コントローラーが外れました。つなぎ直すと続けられます".into();
                }
                Event::GamepadSensorUpdated { sensor: SensorType::Gyroscope, data, .. } => {
                    // SDL のゲームパッドのジャイロ: x がピッチ、y がヨー(ラジアン/秒)
                    gyro = Some((data[0], data[1]));
                }
                _ => {}
            }
        }

        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32().min(0.1);
        last = now;
        // Switch 2 Pro Controller(USB)は、SDL が知らせてくれないので、1秒ごとに探す
        if pad.is_none() && looked_for_switch2.elapsed() >= Duration::from_secs(1) {
            looked_for_switch2 = Instant::now();
            let _ = hid.refresh_devices();
            match switch2::Switch2::open(&hid) {
                Ok(Some(c)) => {
                    notice = "つながりました: Switch 2 Pro Controller(USB)".into();
                    pad = Some(Pad::Switch2(c));
                }
                Ok(None) => {}
                Err(e) => notice = e,
            }
        }
        let input = match &mut pad {
            Some(Pad::Sdl(c, labels)) => Some(read(c, labels, gyro)),
            Some(Pad::Switch2(c)) => match c.poll() {
                Ok(s) => Some(Input { left: s.left, right: s.right, held: s.held.clone(), gyro: None }),
                Err(e) => {
                    disconnected(&mut ctl, &sender, opts.test);
                    pad = None;
                    notice = format!("{e}。つなぎ直すと続けられます");
                    None
                }
            },
            None => None,
        };
        if let Some(input) = input {
            let (msgs, rumble) = ctl.step(&input, dt);
            if !opts.test {
                for m in &msgs {
                    sender.send(m);
                }
            }
            if let (Some(r), Some(Pad::Sdl(c, _))) = (rumble, &mut pad) {
                let v = (r.strength.clamp(0.0, 1.0) * 65535.0) as u16;
                let _ = c.set_rumble(v, v, r.ms);
            }
            if !ctl.last.is_empty() {
                notice = std::mem::take(&mut ctl.last);
            }
            if opts.test && !input.held.is_empty() {
                notice = format!("押している: {:?}", input.held);
            }
        }
        if drawn.elapsed() >= Duration::from_millis(66) {
            draw(&ctl, pad.is_some(), &notice);
            drawn = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    if !opts.test {
        for m in ctl.release_all() {
            sender.send(&m);
        }
    }
    println!("\n終わりました");
    Ok(())
}

/// `--demo`: 決まった操作を、本物と同じ処理に通して送り続ける(Ctrl+C で終わる)
fn run_demo(sender: &osc::Sender, mut ctl: Controller) -> Result<(), String> {
    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = stop.clone();
        let _ = ctrlc::set_handler(move || stop.store(true, Ordering::SeqCst));
    }
    println!("procon-light: デモ。送り先 {}。{}秒でひとまわりします。Ctrl+C で終わる", sender.to(), demo::LOOP_SECS);
    let start = Instant::now();
    let mut last = start;
    let mut drawn = start - Duration::from_secs(1);
    let mut notice = String::new();
    while !stop.load(Ordering::SeqCst) {
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32().min(0.1);
        last = now;
        let (msgs, _) = ctl.step(&demo::input_at(now.duration_since(start).as_secs_f32()), dt);
        for m in &msgs {
            sender.send(m);
        }
        if !ctl.last.is_empty() {
            notice = std::mem::take(&mut ctl.last);
        }
        if drawn.elapsed() >= Duration::from_millis(66) {
            draw(&ctl, true, &notice);
            drawn = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    for m in ctl.release_all() {
        sender.send(&m);
    }
    println!("\n終わりました");
    Ok(())
}

/// つないでいるコントローラー
enum Pad {
    /// SDL3 で読むもの(Pro Controller など)
    Sdl(Gamepad, Labels),
    /// Switch 2 Pro Controller(USB)
    Switch2(switch2::Switch2),
}

/// コントローラーが外れたとき: 押していたフラッシュを戻し、押していないことにする
fn disconnected(ctl: &mut Controller, sender: &osc::Sender, test: bool) {
    if !test {
        for m in ctl.release_all() {
            sender.send(&m);
        }
    }
    ctl.step(&Input::default(), 0.0);
}

/// A・B・X・Y が、どの位置のボタンか。SDL3 はボタンを位置(下・右・左・上)で扱い、
/// 任天堂の並びは Xbox と逆なので、ボタンに書いてある文字から引く
struct Labels([(Button, Btn); 4]);

impl Labels {
    fn of(c: &Gamepad) -> Labels {
        let faces = [Button::South, Button::East, Button::West, Button::North];
        let pick = |want: ButtonLabel, fallback: Button| faces.iter().copied().find(|&b| c.button_label_for_gamepad_type(b) == want).unwrap_or(fallback);
        // 文字が分からないときは、任天堂の並び(右が A、下が B、上が X、左が Y)とみなす
        Labels([
            (pick(ButtonLabel::A, Button::East), Btn::A),
            (pick(ButtonLabel::B, Button::South), Btn::B),
            (pick(ButtonLabel::X, Button::North), Btn::X),
            (pick(ButtonLabel::Y, Button::West), Btn::Y),
        ])
    }
}

/// コントローラーの今の状態を読む
fn read(c: &Gamepad, labels: &Labels, gyro: Option<(f32, f32)>) -> Input {
    let axis = |a: Axis| (c.axis(a) as f32 / 32767.0).clamp(-1.0, 1.0);
    let mut held = Vec::new();
    for (sdl, btn) in labels.0.iter().copied().chain([
        (Button::LeftShoulder, Btn::L),
        (Button::RightShoulder, Btn::R),
        (Button::LeftStick, Btn::LStick),
        (Button::RightStick, Btn::RStick),
        (Button::DPadUp, Btn::Up),
        (Button::DPadDown, Btn::Down),
        (Button::DPadLeft, Btn::Left),
        (Button::DPadRight, Btn::Right),
        (Button::Back, Btn::Minus),
        (Button::Start, Btn::Plus),
        (Button::Misc1, Btn::Capture),
    ]) {
        if c.button(sdl) {
            held.push(btn);
        }
    }
    // Pro Controller の ZL・ZR は押したか押さないかだけだが、SDL では軸として届く(0〜32767)
    if axis(Axis::TriggerLeft) > 0.5 {
        held.push(Btn::ZL);
    }
    if axis(Axis::TriggerRight) > 0.5 {
        held.push(Btn::ZR);
    }
    // SDL は下がプラスなので、上がプラスになるよう向きを変える
    Input { left: (axis(Axis::LeftX), -axis(Axis::LeftY)), right: (axis(Axis::RightX), -axis(Axis::RightY)), held, gyro }
}

/// 端末の1行に、今の様子を出す(色は実際の色で塗る)
fn draw(ctl: &Controller, connected: bool, notice: &str) {
    let (r, g, b) = ctl.rgb();
    let swatch = if ctl.color_on { format!("\x1b[48;2;{r};{g};{b}m      \x1b[0m") } else { "(パレット)".into() };
    let state = if !connected {
        "未接続".to_string()
    } else if ctl.paused {
        "一時停止".to_string()
    } else {
        format!(
            "色 {swatch} {:>3}° {:>3}%{} | 明るさ {:>3}%{} | 向き {:>3}% {:>3}%{}",
            (ctl.hue * 360.0).round() as i32,
            (ctl.sat * 100.0).round() as i32,
            if ctl.auto_hue { " 回転中" } else { "" },
            (ctl.master * 100.0).round() as i32,
            if ctl.faded { " 暗転" } else { "" },
            (ctl.pan * 100.0).round() as i32,
            (ctl.tilt * 100.0).round() as i32,
            if ctl.gyro_aim { " ジャイロ" } else { "" },
        )
    };
    print!("\r\x1b[2K{state} | {notice}");
    let _ = std::io::stdout().flush();
}
