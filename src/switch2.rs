//! Switch 2 Pro Controller(USB でつないだもの)を読む。
//!
//! このコントローラーは、つないだあと始めの合図を送らないと、ボタンなどを送ってこない。
//! 合図は USB の1番の入り口(バルク転送)へ送る。ボタンなどは0番の入り口に HID として届く。
//! SDL3 にも対応する処理があるが、0番と1番の両方を libusb でつかもうとする。Mac では0番を OS がつかんでいて、
//! 管理者の権限が無いと借りられないので、SDL3 では使えない。そこで、合図は libusb で1番へ送り、
//! ボタンなどは OS の HID の仕組み(hidapi)で0番から読む。SDL3 の SDL_hid_* は、この機種を libusb 専用として
//! 一覧から外すので使えない。
//!
//! 合図の中身と、届く値の並びは SDL3 の src/joystick/hidapi/SDL_hidapi_switch2.c と同じにしてある。

use crate::control::Btn;
use std::time::Duration;

pub const VENDOR: u16 = 0x057e;
pub const PRODUCT: u16 = 0x2069;

/// スティック1軸の補正値。neutral は真ん中、min・max は真ん中から端までの幅
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Axis {
    neutral: u16,
    min: u16,
    max: u16,
}

impl Axis {
    /// 生の値(0〜4095)を -1〜1 にする
    fn map(&self, raw: u16) -> f32 {
        let v = raw as f32;
        if self.neutral != 0 && self.min != 0 && self.max != 0 {
            let d = v - self.neutral as f32;
            (if d < 0.0 { d / self.min as f32 } else { d / self.max as f32 }).clamp(-1.0, 1.0)
        } else {
            (v / 2048.0 - 1.0).clamp(-1.0, 1.0)
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Stick {
    x: Axis,
    y: Axis,
}

/// 補正値の並び(9バイト)を読む
fn parse_stick(d: &[u8]) -> Stick {
    let lo = |a: u8, b: u8| a as u16 | ((b as u16 & 0x0f) << 8);
    let hi = |a: u8, b: u8| (a as u16 >> 4) | ((b as u16) << 4);
    Stick {
        x: Axis { neutral: lo(d[0], d[1]), max: lo(d[3], d[4]), min: lo(d[6], d[7]) },
        y: Axis { neutral: hi(d[1], d[2]), max: hi(d[4], d[5]), min: hi(d[7], d[8]) },
    }
}

/// 1回分の状態。スティックは -1〜1 で、上と右がプラス
#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub left: (f32, f32),
    pub right: (f32, f32),
    pub held: Vec<Btn>,
}

/// 届いた値の並び(先頭はレポートの番号。USB では 0x05 が届く)から、状態を読む。
/// SDL3 と同じく番号は見ず、64バイトあるものを読む
fn parse_report(d: &[u8], left: &Stick, right: &Stick) -> Option<State> {
    if d.len() < 64 {
        return None;
    }
    let mut held = Vec::new();
    let bits: [(usize, u8, Btn); 18] = [
        (5, 0x01, Btn::Y),
        (5, 0x02, Btn::X),
        (5, 0x04, Btn::B),
        (5, 0x08, Btn::A),
        (5, 0x40, Btn::R),
        (5, 0x80, Btn::ZR),
        (6, 0x01, Btn::Minus),
        (6, 0x02, Btn::Plus),
        (6, 0x04, Btn::RStick),
        (6, 0x08, Btn::LStick),
        (6, 0x10, Btn::Home),
        (6, 0x20, Btn::Capture),
        (7, 0x01, Btn::Down),
        (7, 0x02, Btn::Up),
        (7, 0x04, Btn::Right),
        (7, 0x08, Btn::Left),
        (7, 0x40, Btn::L),
        (7, 0x80, Btn::ZL),
    ];
    for (i, mask, b) in bits {
        if d[i] & mask != 0 {
            held.push(b);
        }
    }
    let x = |a: u8, b: u8| a as u16 | ((b as u16 & 0x0f) << 8);
    let y = |a: u8, b: u8| (a as u16 >> 4) | ((b as u16) << 4);
    Some(State {
        left: (left.x.map(x(d[11], d[12])), left.y.map(y(d[12], d[13]))),
        right: (right.x.map(x(d[14], d[15])), right.y.map(y(d[15], d[16]))),
        held,
    })
}

pub struct Switch2 {
    hid: hidapi::HidDevice,
    left: Stick,
    right: Stick,
    last: State,
    // 1番の入り口は、つないでいる間つかんだままにする(離すと、ほかのアプリが合図を送り直せてしまう)
    _usb: rusb::DeviceHandle<rusb::GlobalContext>,
}

/// 合図の並び(SDL3 と同じ)。それぞれの6バイト目に、9バイト目からの長さが入っている
const INIT: &[&[u8]] = &[
    &[0x07, 0x91, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00],
    // 機能の出力の設定
    &[0x0c, 0x91, 0x00, 0x02, 0x00, 0x04, 0x00, 0x00, 0x27, 0x00, 0x00, 0x00],
    &[0x11, 0x91, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00],
    // 振動の設定
    &[
        0x0a, 0x91, 0x00, 0x08, 0x00, 0x14, 0x00, 0x00, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x35, 0x00, 0x46, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ],
    // 機能の出力を ON
    &[0x0c, 0x91, 0x00, 0x04, 0x00, 0x04, 0x00, 0x00, 0x27, 0x00, 0x00, 0x00],
    &[0x01, 0x91, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00],
    // 振動を ON
    &[0x01, 0x91, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00],
    // 充電グリップのボタンを ON
    &[0x08, 0x91, 0x00, 0x02, 0x00, 0x04, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00],
    // 届く値の並びを決める
    &[0x03, 0x91, 0x00, 0x0a, 0x00, 0x04, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00],
    // 送り始める
    &[0x03, 0x91, 0x00, 0x0d, 0x00, 0x08, 0x00, 0x00, 0x01, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
];

const TIMEOUT: Duration = Duration::from_millis(1000);

struct Bulk {
    usb: rusb::DeviceHandle<rusb::GlobalContext>,
    out_ep: u8,
    in_ep: u8,
}

impl Bulk {
    fn send(&self, data: &[u8]) -> rusb::Result<usize> {
        self.usb.write_bulk(self.out_ep, data, TIMEOUT)
    }

    /// `size` バイトまで受け取る(64バイトずつ届く)
    fn recv(&self, size: usize) -> rusb::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(size);
        while out.len() < size {
            let mut buf = [0u8; 64];
            let want = (size - out.len()).min(64);
            let n = self.usb.read_bulk(self.in_ep, &mut buf[..want], TIMEOUT)?;
            out.extend_from_slice(&buf[..n]);
            if n < want {
                break;
            }
        }
        Ok(out)
    }

    /// 本体の記憶から 0x40 バイトを読む(補正値など)
    fn read_flash(&self, address: u32) -> Option<Vec<u8>> {
        let mut cmd = [0x02, 0x91, 0x00, 0x01, 0x00, 0x08, 0x00, 0x00, 0, 0, 0, 0, 0, 0, 0, 0];
        cmd[12..16].copy_from_slice(&address.to_le_bytes());
        self.send(&cmd).ok()?;
        let r = self.recv(0x50).ok()?;
        (r.len() >= 0x50).then(|| r[0x10..0x50].to_vec())
    }
}

impl Switch2 {
    /// つながっていれば、合図を送って開く。つながっていなければ Ok(None)
    pub fn open(api: &hidapi::HidApi) -> Result<Option<Switch2>, String> {
        let Some(usb) = rusb::open_device_with_vid_pid(VENDOR, PRODUCT) else { return Ok(None) };
        let (out_ep, in_ep) = bulk_endpoints(&usb.device()).ok_or("Switch 2 Pro Controller のバルクの入り口が見つかりません")?;
        let _ = usb.set_auto_detach_kernel_driver(true);
        usb.claim_interface(1).map_err(|e| format!("Switch 2 Pro Controller の1番の入り口をつかめません: {e}(ほかのアプリが使っていないか確かめてください)"))?;
        let bulk = Bulk { usb, out_ep, in_ep };

        // スティックの補正値。工場の値のあと、利用者が本体で補正した値があればそちらを使う
        let mut left = bulk.read_flash(0x13080).map(|d| parse_stick(&d[0x28..])).unwrap_or_default();
        let mut right = bulk.read_flash(0x130c0).map(|d| parse_stick(&d[0x28..])).unwrap_or_default();
        if let Some(d) = bulk.read_flash(0x1fc040).filter(|d| d[0] == 0xb2 && d[1] == 0xa1) {
            left = parse_stick(&d[2..]);
        }
        if let Some(d) = bulk.read_flash(0x1fc080).filter(|d| d[0] == 0xb2 && d[1] == 0xa1) {
            right = parse_stick(&d[2..]);
        }
        for cmd in INIT {
            let len = cmd[5] as usize + 8;
            bulk.send(&cmd[..len.min(cmd.len())]).map_err(|e| format!("Switch 2 Pro Controller に合図を送れません: {e}"))?;
            let _ = bulk.recv(0x40);
        }

        let hid = api
            .device_list()
            .find(|d| d.vendor_id() == VENDOR && d.product_id() == PRODUCT && d.interface_number() <= 0)
            .ok_or("Switch 2 Pro Controller の HID が見つかりません")?
            .open_device(api)
            .map_err(|e| format!("Switch 2 Pro Controller の HID を開けません: {e}"))?;
        hid.set_blocking_mode(false).map_err(|e| e.to_string())?;
        Ok(Some(Switch2 { hid, left, right, last: State::default(), _usb: bulk.usb }))
    }

    /// 届いている値をすべて読み、最新の状態を返す。外れたら Err
    pub fn poll(&mut self) -> Result<&State, String> {
        let mut buf = [0u8; 64];
        loop {
            match self.hid.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if std::env::var_os("PROCON_LIGHT_DUMP").is_some() {
                        eprintln!("{:02x?}", &buf[..n.min(20)]);
                    }
                    if let Some(s) = parse_report(&buf[..n], &self.left, &self.right) {
                        self.last = s;
                    }
                }
                Err(e) => return Err(format!("Switch 2 Pro Controller が外れました: {e}")),
            }
        }
        Ok(&self.last)
    }
}

fn bulk_endpoints(dev: &rusb::Device<rusb::GlobalContext>) -> Option<(u8, u8)> {
    let config = dev.config_descriptor(0).ok()?;
    let (mut out_ep, mut in_ep) = (None, None);
    for iface in config.interfaces() {
        for alt in iface.descriptors().filter(|a| a.interface_number() == 1) {
            for ep in alt.endpoint_descriptors().filter(|e| e.transfer_type() == rusb::TransferType::Bulk) {
                match ep.direction() {
                    rusb::Direction::Out => out_ep = Some(ep.address()),
                    rusb::Direction::In => in_ep = Some(ep.address()),
                }
            }
        }
    }
    Some((out_ep?, in_ep?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calibration_maps_neutral_to_zero_and_edges_to_one() {
        let a = Axis { neutral: 2000, min: 1500, max: 1600 };
        assert_eq!(a.map(2000), 0.0);
        assert_eq!(a.map(3600), 1.0);
        assert_eq!(a.map(500), -1.0);
        // 補正値が無いときは、0〜4095 をそのまま -1〜1 に
        assert!(Axis::default().map(2048).abs() < 1e-3);
    }

    #[test]
    fn parses_buttons_and_sticks() {
        let mut d = [0u8; 64];
        d[0] = 0x05;
        d[5] = 0x08 | 0x80; // A と ZR
        d[6] = 0x20; // キャプチャ
        d[7] = 0x02; // 上
        // 左スティック x=4095(右いっぱい) y=2048、右スティック x=2048 y=0(下いっぱい)
        let (lx, ly, rx, ry): (u16, u16, u16, u16) = (4095, 2048, 2048, 0);
        d[11] = lx as u8;
        d[12] = ((lx >> 8) as u8 & 0x0f) | ((ly as u8 & 0x0f) << 4);
        d[13] = (ly >> 4) as u8;
        d[14] = rx as u8;
        d[15] = ((rx >> 8) as u8 & 0x0f) | ((ry as u8 & 0x0f) << 4);
        d[16] = (ry >> 4) as u8;
        let s = parse_report(&d, &Stick::default(), &Stick::default()).unwrap();
        assert_eq!(s.held, vec![Btn::A, Btn::ZR, Btn::Capture, Btn::Up]);
        assert!(s.left.0 > 0.99 && s.left.1.abs() < 1e-3);
        assert!(s.right.0.abs() < 1e-3 && s.right.1 < -0.99);
    }

    #[test]
    fn parses_stick_calibration_nibbles() {
        // x の真ん中 0x7d0(2000)、y の真ん中 0x7e0(2016)
        let d = [0xd0, 0x07 | 0x00, 0x7e, 0, 0, 0, 0, 0, 0];
        let s = parse_stick(&d);
        assert_eq!(s.x.neutral, 2000);
        assert_eq!(s.y.neutral, 0x7e0);
    }
}
