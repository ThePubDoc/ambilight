//! Where frames go: a USB serial controller (Adalight) or a WLED controller over Wi-Fi.

use crate::adalight;
use crate::serial::{self, LinkError};
use serialport::SerialPort;
use std::io::{self, Write};
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};

/// WLED's UDP realtime port and the DNRGB protocol (start index + RGB, any LED count).
const WLED_PORT: u16 = 21324;
const DNRGB: u8 = 4;
/// Seconds WLED waits after the last packet before returning to its own effect.
const WLED_TIMEOUT_S: u8 = 2;
const WLED_LEDS_PER_PACKET: usize = 489;

pub enum Link {
    Serial(Box<dyn SerialPort>),
    Wled { socket: UdpSocket, addr: SocketAddr },
}

impl Link {
    pub fn open_serial(path: &str, baud: u32) -> Result<Link, LinkError> {
        serial::open(path, baud).map(Link::Serial)
    }

    /// `host` may be an IP address or a name like `wled.local`.
    pub fn open_wled(host: &str) -> Result<Link, String> {
        let host = host.trim();
        if host.is_empty() {
            return Err("Enter the WLED controller's address in Settings".into());
        }
        let target = if host.contains(':') { host.to_string() } else { format!("{host}:{WLED_PORT}") };
        let addr = target
            .to_socket_addrs()
            .map_err(|e| format!("Can't find {host}: {e}"))?
            .find(|a| a.is_ipv4())
            .ok_or_else(|| format!("Can't find {host}"))?;
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
        Ok(Link::Wled { socket, addr })
    }

    /// Sends RGB bytes (3 per LED). `scratch` is reused between frames.
    pub fn send(&mut self, rgb: &[u8], scratch: &mut Vec<u8>) -> io::Result<()> {
        match self {
            Link::Serial(port) => {
                adalight::frame(rgb, scratch);
                port.write_all(scratch)
            }
            Link::Wled { socket, addr } => {
                for packet in wled_packets(rgb) {
                    socket.send_to(&packet, *addr)?;
                }
                Ok(())
            }
        }
    }
}

/// DNRGB packets: [4, timeout, start index (u16 BE), RGB…], at most 489 LEDs each.
pub fn wled_packets(rgb: &[u8]) -> Vec<Vec<u8>> {
    rgb.chunks(WLED_LEDS_PER_PACKET * 3)
        .enumerate()
        .map(|(i, chunk)| {
            let start = (i * WLED_LEDS_PER_PACKET) as u16;
            let mut p = Vec::with_capacity(4 + chunk.len());
            p.extend_from_slice(&[DNRGB, WLED_TIMEOUT_S]);
            p.extend_from_slice(&start.to_be_bytes());
            p.extend_from_slice(chunk);
            p
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_strip_is_one_packet() {
        let packets = wled_packets(&[9; 139 * 3]);
        assert_eq!(packets.len(), 1);
        assert_eq!(&packets[0][..4], &[4, 2, 0, 0]);
        assert_eq!(packets[0].len(), 4 + 139 * 3);
    }

    #[test]
    fn long_strip_is_split_with_start_index() {
        let packets = wled_packets(&[0; 600 * 3]);
        assert_eq!(packets.len(), 2);
        assert_eq!(&packets[1][2..4], &489u16.to_be_bytes());
        assert_eq!(packets[1].len(), 4 + 111 * 3);
    }

    #[test]
    fn empty_host_is_an_error() {
        assert!(Link::open_wled("  ").is_err());
    }
}
