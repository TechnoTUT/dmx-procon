//! OSC を UDP で送るだけの、小さな送り手。dmxctl の OSC 受信(既定 UDP 9000)へ送る

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    F(f32),
    S(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Msg {
    pub addr: String,
    pub args: Vec<Arg>,
}

impl Msg {
    pub fn new(addr: &str, args: Vec<Arg>) -> Self {
        Msg { addr: addr.into(), args }
    }

    /// 数値を1つだけ付けたもの
    pub fn f(addr: &str, v: f32) -> Self {
        Msg::new(addr, vec![Arg::F(v)])
    }

    /// OSC 1.0 の形にする(文字列は4バイト境界まで0で埋める。数値はビッグエンディアン)
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        pad_str(&mut out, &self.addr);
        let tags: String = std::iter::once(',').chain(self.args.iter().map(|a| match a {
            Arg::F(_) => 'f',
            Arg::S(_) => 's',
        })).collect();
        pad_str(&mut out, &tags);
        for a in &self.args {
            match a {
                Arg::F(v) => out.extend_from_slice(&v.to_be_bytes()),
                Arg::S(s) => pad_str(&mut out, s),
            }
        }
        out
    }
}

fn pad_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(s.as_bytes());
    out.push(0);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

pub struct Sender {
    sock: UdpSocket,
    to: SocketAddr,
}

impl Sender {
    pub fn new(to: &str) -> std::io::Result<Self> {
        let to = to
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, format!("送り先が見つかりません: {to}")))?;
        let sock = UdpSocket::bind(if to.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" })?;
        Ok(Sender { sock, to })
    }

    pub fn to(&self) -> SocketAddr {
        self.to
    }

    /// 届いたかは分からない(UDP)。dmxctl が動いていなくてもエラーにしない
    pub fn send(&self, m: &Msg) {
        let _ = self.sock.send_to(&m.encode(), self.to);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_floats_and_strings_with_padding() {
        let m = Msg::new("/dmx/effect", vec![Arg::S("ブレス".into()), Arg::F(1.0)]);
        let b = m.encode();
        assert_eq!(b.len() % 4, 0);
        // "/dmx/effect" は11バイト → 0を1つ足して12
        assert_eq!(&b[..12], b"/dmx/effect\0");
        // 型の並び ",sf" → 0を1つ足して4
        assert_eq!(&b[12..16], b",sf\0");
        // 最後の4バイトが 1.0 のビッグエンディアン
        assert_eq!(&b[b.len() - 4..], &1.0f32.to_be_bytes());
    }

    #[test]
    fn a_message_without_args_still_has_a_type_tag() {
        assert_eq!(Msg::new("/dmx/tap", vec![]).encode(), b"/dmx/tap\0\0\0\0,\0\0\0".to_vec());
    }
}
