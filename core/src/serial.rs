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
    let mut ports = usb_ports();
    ports.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.path.cmp(&b.1.path)));
    ports.into_iter().map(|(_, p)| p).collect()
}

/// The port for the "Automatic" setting.
pub fn auto_port() -> Result<String, LinkError> {
    pick_auto(usb_ports())
}

/// Automatic only connects when exactly one connected device has an LED-controller chip. Opening
/// a serial port resets many boards and the strip data would reach whatever is listening, so with
/// several candidates (say an LED controller and a 3D printer) the user has to choose.
fn pick_auto(ports: Vec<(bool, PortInfo)>) -> Result<String, LinkError> {
    let mut candidates = ports.into_iter().filter(|(known, _)| *known).map(|(_, p)| p.path);
    match (candidates.next(), candidates.next()) {
        (Some(path), None) => Ok(path),
        (None, _) => Err(LinkError::NotFound),
        (Some(_), Some(_)) => Err(LinkError::Ambiguous),
    }
}

/// Each USB serial port, with whether its chip is one LED controllers commonly use.
fn usb_ports() -> Vec<(bool, PortInfo)> {
    serialport::available_ports()
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
        .collect()
}

pub fn short_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkError {
    NotFound,
    /// "Automatic" found more than one possible LED controller.
    Ambiguous,
    Busy,
    Other(String),
}

impl fmt::Display for LinkError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LinkError::NotFound => write!(f, "LED controller not found. Check the USB cable."),
            LinkError::Ambiguous => {
                write!(f, "Several USB serial devices found. Choose the LED controller under Settings → Connection.")
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn port(path: &str, known: bool) -> (bool, PortInfo) {
        (known, PortInfo { path: path.into(), label: path.into() })
    }

    #[test]
    fn automatic_picks_the_only_led_controller() {
        let ports = vec![port("/dev/cu.usbmodem1", false), port("/dev/cu.usbserial-1110", true)];
        assert_eq!(pick_auto(ports), Ok("/dev/cu.usbserial-1110".into()));
    }

    #[test]
    fn automatic_never_opens_an_unknown_device() {
        assert_eq!(pick_auto(vec![port("/dev/cu.usbmodem1", false)]), Err(LinkError::NotFound));
        assert_eq!(pick_auto(vec![]), Err(LinkError::NotFound));
    }

    #[test]
    fn automatic_asks_when_several_devices_could_be_the_controller() {
        let ports = vec![port("/dev/cu.wchusbserialA", true), port("/dev/cu.wchusbserialB", true)];
        assert_eq!(pick_auto(ports), Err(LinkError::Ambiguous));
    }
}
