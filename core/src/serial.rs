use serde::Serialize;
use serialport::{SerialPort, SerialPortType};
use std::fmt;
use std::time::Duration;

/// USB vendor IDs of chips commonly found on Adalight-style controllers
/// (WCH CH340, Silicon Labs CP210x, FTDI, Arduino, Espressif).
const KNOWN_VENDORS: [u16; 5] = [0x1a86, 0x10c4, 0x0403, 0x2341, 0x303a];

#[derive(Debug, Clone, Serialize)]
pub struct PortInfo {
    /// Path or name to open (`/dev/cu.usbserial-1110`, `COM3`).
    pub path: String,
    /// Human-readable label for menus.
    pub label: String,
}

/// USB serial ports, likely LED controllers first.
pub fn available_ports() -> Vec<PortInfo> {
    let mut ports: Vec<(bool, PortInfo)> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        // macOS lists each device twice; /dev/cu.* is the one to open for output.
        .filter(|p| !p.port_name.starts_with("/dev/tty."))
        .filter_map(|p| match p.port_type {
            SerialPortType::UsbPort(usb) => {
                let name = short_name(&p.port_name);
                let label = match usb.product.as_deref() {
                    Some(product) if !product.is_empty() => format!("{name} ({product})"),
                    _ => name.to_string(),
                };
                Some((KNOWN_VENDORS.contains(&usb.vid), PortInfo { path: p.port_name, label }))
            }
            _ => None,
        })
        .collect();
    ports.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.path.cmp(&b.1.path)));
    ports.into_iter().map(|(_, p)| p).collect()
}

pub fn short_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkError {
    NotFound,
    Busy,
    Other(String),
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LinkError::NotFound => write!(f, "LED controller not found. Check the USB cable."),
            LinkError::Busy => write!(f, "USB port is in use by another app"),
            LinkError::Other(msg) => write!(f, "Can't open the USB port: {msg}"),
        }
    }
}

pub fn open(path: &str, baud: u32) -> Result<Box<dyn SerialPort>, LinkError> {
    serialport::new(path, baud).timeout(Duration::from_millis(250)).open().map_err(|e| {
        let text = e.to_string();
        let lower = text.to_lowercase();
        match e.kind() {
            serialport::ErrorKind::NoDevice => LinkError::NotFound,
            serialport::ErrorKind::Io(std::io::ErrorKind::NotFound) => LinkError::NotFound,
            serialport::ErrorKind::Io(std::io::ErrorKind::PermissionDenied) => LinkError::Busy,
            _ if lower.contains("busy") || lower.contains("access is denied") => LinkError::Busy,
            _ if lower.contains("no such file") || lower.contains("cannot find") => LinkError::NotFound,
            _ => LinkError::Other(text),
        }
    })
}
