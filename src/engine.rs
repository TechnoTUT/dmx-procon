//! コントローラーを読み、操作を OSC にして送り続ける部分。ウィンドウ(gui.rs)からも端末(--cli)からも使う。
//! 画面とは `Shared` でやりとりする: 画面は設定(送り先・テスト)を書き、ここは今の様子(`Status`)を書く。

use crate::control::{Btn, Controller, Input};
use crate::{demo, osc, rig, switch2};
use sdl3::event::Event;
use sdl3::gamepad::{Axis, Button, ButtonLabel, Gamepad};
use sdl3::sensor::SensorType;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// 画面から変えられる設定
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// dmxctl の OSC 受信の送り先(HOST:PORT)
    pub to: String,
    /// 送らずに、入力だけを見る
    pub test: bool,
}

/// 色・明るさ・向きなど、今の様子(Controller の写し)
#[derive(Debug, Clone, Default)]
pub struct View {
    pub rgb: (u8, u8, u8),
    pub color_on: bool,
    pub hue: f32,
    pub sat: f32,
    pub auto_hue: bool,
    pub master: f32,
    pub faded: bool,
    pub paused: bool,
    pub gyro_aim: bool,
    pub pan: f32,
    pub tilt: f32,
    pub move_on: bool,
    pub gobo: Option<String>,
    pub movers_dark: bool,
    pub laser: bool,
}

impl View {
    fn of(c: &Controller) -> View {
        View {
            rgb: c.rgb(),
            color_on: c.color_on,
            hue: c.hue,
            sat: c.sat,
            auto_hue: c.auto_hue,
            master: c.master,
            faded: c.faded,
            paused: c.paused,
            gyro_aim: c.gyro_aim,
            pan: c.pan,
            tilt: c.tilt,
            move_on: c.move_on,
            gobo: c.gobo_index.and_then(|i| c.gobos.get(i)).map(|g| g.0.clone()),
            movers_dark: c.movers_dark,
            laser: c.laser,
        }
    }
}

/// 画面に出す、今の様子
#[derive(Debug, Clone, Default)]
pub struct Status {
    /// つないでいるコントローラーの名前
    pub pad: Option<String>,
    /// 最後にした操作や、知らせ
    pub notice: String,
    pub view: View,
    /// 押しているボタンと、スティック(-1〜1、上と右がプラス)
    pub held: Vec<Btn>,
    pub left: (f32, f32),
    pub right: (f32, f32),
    /// 送れなかったときの理由
    pub error: Option<String>,
    /// dmxctl の設定から見つけたもの
    pub movers: Vec<u32>,
    pub pixel_fx: usize,
    /// dmxctl の設定で、OSC 受信が OFF になっている
    pub osc_off: bool,
    pub demo: bool,
}

pub struct Shared {
    pub settings: Mutex<Settings>,
    pub status: Mutex<Status>,
    pub stop: AtomicBool,
}

pub fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// 起動のしかた(コマンドラインで指定するもの)
#[derive(Debug, Clone, Default)]
pub struct Options {
    pub to: Option<String>,
    pub movers: Option<Vec<u32>>,
    pub test: bool,
    pub demo: bool,
}

/// dmxctl の設定を読み、操作の決め手(Controller)と、はじめの設定・様子を作る
pub fn setup(opts: &Options, saved_to: Option<String>) -> (Controller, Shared) {
    let dir = rig::config_dir();
    let project: Option<serde_json::Value> = std::fs::read_to_string(dir.join("project.json")).ok().and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok());
    let (found, notice) = match rig::Rig::load(&dir) {
        Ok(r) => (r, String::new()),
        Err(e) => (rig::Rig::default(), format!("dmxctl の設定を読めませんでした({e})。ムービングと LED バーのエフェクトは使えません")),
    };
    let osc_port = project.as_ref().and_then(|p| p["osc_port"].as_u64()).unwrap_or(9000);
    let osc_off = project.as_ref().is_some_and(|p| p["osc_enabled"] == false);
    let movers = opts.movers.clone().unwrap_or(found.movers);
    let to = opts.to.clone().or(saved_to).unwrap_or_else(|| format!("127.0.0.1:{osc_port}"));
    let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
    let ctl = Controller::new(movers.clone(), found.pixel_fx.clone(), seed).with_mover_info(found.dimmers, found.gobos);
    let status = Status { notice, movers, pixel_fx: found.pixel_fx.len(), osc_off, demo: opts.demo, view: View::of(&ctl), ..Default::default() };
    let shared = Shared { settings: Mutex::new(Settings { to, test: opts.test }), status: Mutex::new(status), stop: AtomicBool::new(false) };
    (ctl, shared)
}

/// 送り先が変わったら、作り直す送り口
struct Out {
    to: String,
    sender: Result<osc::Sender, String>,
}

impl Out {
    fn new(to: &str) -> Out {
        Out { to: to.to_string(), sender: osc::Sender::new(to).map_err(|e| format!("送り先が使えません({to}): {e}")) }
    }

    fn send(&self, msgs: &[osc::Msg]) {
        if let Ok(s) = &self.sender {
            for m in msgs {
                s.send(m);
            }
        }
    }
}

/// `stop` が立つまで、コントローラーを読んで送り続ける。様子が変わると `changed` を呼ぶ(多くても1秒に15回)
pub fn run(shared: &Shared, mut ctl: Controller, changed: &dyn Fn()) -> Result<(), String> {
    let demo_mode = lock(&shared.status).demo;
    let mut out = Out::new(&lock(&shared.settings).to);

    // SDL の設定は、初期化の前に決める
    // ウィンドウが手前に無いときも入力を受ける(DJ の最中は、ほかのアプリが手前にあることが多い)
    sdl3::hint::set("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
    sdl3::hint::set("SDL_JOYSTICK_HIDAPI", "1");
    sdl3::hint::set("SDL_JOYSTICK_HIDAPI_SWITCH", "1");
    // Switch 2 Pro Controller は src/switch2.rs で読む。SDL には触らせない
    sdl3::hint::set("SDL_JOYSTICK_HIDAPI_SWITCH2", "0");
    sdl3::hint::set("SDL_HIDAPI_LIBUSB", "0");
    let sdl = if demo_mode { None } else { Some(sdl3::init().map_err(|e| e.to_string())?) };
    let gc = sdl.as_ref().map(|s| s.gamepad()).transpose().map_err(|e| e.to_string())?;
    let mut events = sdl.as_ref().map(|s| s.event_pump()).transpose().map_err(|e| e.to_string())?;
    let mut hid = if demo_mode { None } else { Some(hidapi::HidApi::new().map_err(|e| format!("HID を使えません: {e}"))?) };

    let mut pad: Option<Pad> = None;
    let mut looked_for_switch2 = Instant::now() - Duration::from_secs(10);
    let mut gyro: Option<(f32, f32)> = None;
    let start = Instant::now();
    let mut last = start;
    let mut reported = start - Duration::from_secs(1);
    {
        let mut s = lock(&shared.status);
        if s.notice.is_empty() {
            s.notice = if demo_mode { "デモ: 決まった操作を送っています".into() } else { "コントローラーを待っています".into() };
        }
    }

    while !shared.stop.load(Ordering::SeqCst) {
        let settings = lock(&shared.settings).clone();
        if settings.to != out.to {
            out = Out::new(&settings.to);
        }
        let mut notice: Option<String> = None;
        let send = |out: &Out, msgs: &[osc::Msg]| {
            if !settings.test {
                out.send(msgs);
            }
        };

        if let (Some(events), Some(gc)) = (events.as_mut(), gc.as_ref()) {
            for ev in events.poll_iter() {
                match ev {
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
                            let name = format!("{}{}", c.name().unwrap_or_default(), if has_gyro { "" } else { "(ジャイロなし)" });
                            notice = Some(format!("つながりました: {name}"));
                            lock(&shared.status).pad = Some(name);
                            pad = Some(Pad::Sdl(c, labels));
                        }
                        Err(e) => notice = Some(format!("コントローラーを開けません: {e}")),
                    },
                    Event::GamepadRemoved { which, .. } if matches!(&pad, Some(Pad::Sdl(c, _)) if c.id().ok() == Some(which)) => {
                        send(&out, &ctl.release_all());
                        ctl.step(&Input::default(), 0.0);
                        pad = None;
                        gyro = None;
                        lock(&shared.status).pad = None;
                        notice = Some("コントローラーが外れました。つなぎ直すと続けられます".into());
                    }
                    Event::GamepadSensorUpdated { sensor: SensorType::Gyroscope, data, .. } => {
                        // SDL のゲームパッドのジャイロ: x がピッチ、y がヨー(ラジアン/秒)
                        gyro = Some((data[0], data[1]));
                    }
                    _ => {}
                }
            }
        }

        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f32().min(0.1);
        last = now;
        // Switch 2 Pro Controller(USB)は、SDL が知らせてくれないので、1秒ごとに探す
        if let Some(hid) = hid.as_mut()
            && pad.is_none()
            && looked_for_switch2.elapsed() >= Duration::from_secs(1)
        {
            looked_for_switch2 = Instant::now();
            let _ = hid.refresh_devices();
            match switch2::Switch2::open(hid) {
                Ok(Some(c)) => {
                    notice = Some("つながりました: Switch 2 Pro Controller(USB)".into());
                    lock(&shared.status).pad = Some("Switch 2 Pro Controller(USB)".into());
                    pad = Some(Pad::Switch2(c));
                }
                Ok(None) => {}
                Err(e) => notice = Some(e),
            }
        }
        let input = if demo_mode {
            Some(demo::input_at(now.duration_since(start).as_secs_f32()))
        } else {
            match &mut pad {
                Some(Pad::Sdl(c, labels)) => Some(read(c, labels, gyro)),
                Some(Pad::Switch2(c)) => match c.poll() {
                    Ok(s) => Some(Input { left: s.left, right: s.right, held: s.held.clone(), gyro: s.gyro }),
                    Err(e) => {
                        send(&out, &ctl.release_all());
                        ctl.step(&Input::default(), 0.0);
                        pad = None;
                        lock(&shared.status).pad = None;
                        notice = Some(format!("{e}。つなぎ直すと続けられます"));
                        None
                    }
                },
                None => None,
            }
        };
        let held = input.as_ref().map(|i| i.held.clone()).unwrap_or_default();
        let sticks = input.as_ref().map(|i| (i.left, i.right)).unwrap_or_default();
        if let Some(input) = input {
            let (msgs, rumble) = ctl.step(&input, dt);
            send(&out, &msgs);
            if let (Some(r), Some(Pad::Sdl(c, _))) = (rumble, &mut pad) {
                let v = (r.strength.clamp(0.0, 1.0) * 65535.0) as u16;
                let _ = c.set_rumble(v, v, r.ms);
            }
            if !ctl.last.is_empty() {
                notice = Some(std::mem::take(&mut ctl.last));
            }
        }

        if notice.is_some() || reported.elapsed() >= Duration::from_millis(66) {
            {
                let mut s = lock(&shared.status);
                if let Some(n) = notice {
                    s.notice = n;
                }
                s.view = View::of(&ctl);
                s.held = held;
                (s.left, s.right) = sticks;
                s.error = out.sender.as_ref().err().cloned();
            }
            changed();
            reported = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    // 押していたフラッシュ・暗くしていたムービングを戻してから終わる
    if !lock(&shared.settings).test {
        out.send(&ctl.release_all());
    }
    Ok(())
}

/// つないでいるコントローラー
enum Pad {
    /// SDL3 で読むもの(Pro Controller など)
    Sdl(Gamepad, Labels),
    /// Switch 2 Pro Controller(USB)
    Switch2(switch2::Switch2),
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
        (Button::Guide, Btn::Home),
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
