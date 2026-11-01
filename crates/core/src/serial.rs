//! Serial ports (R1 to R7): listing, opening, and a terminal on one.
//!
//! One task owns the open port. It reads, writes what the terminal sends, and changes the
//! control lines, so the port is never used from two places at once.

use std::io;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Serialize;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, oneshot};
use tokio_serial::{SerialPort, SerialPortBuilderExt, SerialPortType, SerialStream};
use tokio_util::sync::CancellationToken;

use crate::error::{AppError, ErrorCode};
use crate::sessions::{FlowControl, LineEnding, Parity, SavedSession};
use crate::terminal::{
    BackendEvent, MAX_BACKEND_READ, OutputSink, PortControl, PumpConfig, QUEUE_SIZE, Session,
    SessionRegistry, TerminalBackend, TerminalStatus, spawn_pump,
};

/// How long a break is held. Devices notice anything over about 100 ms.
const BREAK_FOR: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PortInfo {
    /// What is passed to open it: `COM3` or `/dev/ttyUSB0`.
    pub name: String,
    /// A short description for people, such as the USB device's name. May be empty.
    pub label: String,
}

/// Text from a device's own descriptors is untrusted: control characters are dropped and it is
/// cut short.
fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect::<String>()
        .trim()
        .to_string()
}

fn label_of(kind: &SerialPortType) -> String {
    match kind {
        SerialPortType::UsbPort(usb) => {
            let words: Vec<String> = [usb.manufacturer.as_deref(), usb.product.as_deref()]
                .into_iter()
                .flatten()
                .map(plain)
                .filter(|w| !w.is_empty())
                .collect();
            if words.is_empty() {
                format!("USB {:04x}:{:04x}", usb.vid, usb.pid)
            } else {
                words.join(" ")
            }
        }
        SerialPortType::BluetoothPort => "Bluetooth".into(),
        SerialPortType::PciPort => "Built-in".into(),
        SerialPortType::Unknown => String::new(),
    }
}

/// The serial ports this computer has right now.
pub async fn list_ports() -> Result<Vec<PortInfo>, AppError> {
    let found = tokio::task::spawn_blocking(tokio_serial::available_ports)
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't look for serial ports."))?
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't look for serial ports."))?;
    let mut ports: Vec<PortInfo> = found
        .into_iter()
        .map(|p| PortInfo {
            label: label_of(&p.port_type),
            name: p.port_name,
        })
        .collect();
    ports.sort_by_key(|p| natural(&p.name));
    Ok(ports)
}

/// Sorts COM2 before COM10.
fn natural(name: &str) -> (String, u32) {
    let digits: String = name
        .chars()
        .rev()
        .take_while(char::is_ascii_digit)
        .collect();
    let number = digits
        .chars()
        .rev()
        .collect::<String>()
        .parse()
        .unwrap_or(0);
    (
        name.trim_end_matches(|c: char| c.is_ascii_digit())
            .to_string(),
        number,
    )
}

/// How to open a port and treat what is typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialConfig {
    pub port: String,
    pub baud: u32,
    pub data_bits: u8,
    pub parity: Parity,
    pub stop_bits: u8,
    pub flow_control: FlowControl,
    pub local_echo: bool,
    pub line_ending: LineEnding,
}

impl SerialConfig {
    /// The settings of a saved serial session, or an error if it isn't one.
    pub fn from_saved(session: &SavedSession) -> Result<Self, AppError> {
        match session {
            SavedSession::Serial {
                port,
                baud,
                data_bits,
                parity,
                stop_bits,
                flow_control,
                local_echo,
                line_ending,
                ..
            } => Ok(Self {
                port: port.trim().to_string(),
                baud: *baud,
                data_bits: *data_bits,
                parity: *parity,
                stop_bits: *stop_bits,
                flow_control: *flow_control,
                local_echo: *local_echo,
                line_ending: *line_ending,
            }),
            _ => Err(AppError::new(
                ErrorCode::InvalidInput,
                "That isn't a saved serial session.",
            )),
        }
    }
}

fn open_error(port: &str, error: &tokio_serial::Error) -> AppError {
    use tokio_serial::ErrorKind;
    match error.kind() {
        ErrorKind::NoDevice => AppError::new(
            ErrorCode::NotFound,
            format!("No device was found at {port}. Is it plugged in?"),
        ),
        ErrorKind::Io(io::ErrorKind::NotFound) => AppError::new(
            ErrorCode::NotFound,
            format!("{port} doesn't exist. Is the device plugged in?"),
        ),
        ErrorKind::Io(io::ErrorKind::PermissionDenied) => permission_denied(port),
        _ => AppError::new(
            ErrorCode::ConnectionFailed,
            format!("Couldn't open {port}. Another program may be using it."),
        ),
    }
}

/// R4: on Linux, the usual reason is not being in the `dialout` group. The command is only
/// shown, never run.
fn permission_denied(port: &str) -> AppError {
    let message = if cfg!(target_os = "linux") {
        format!(
            "You don't have permission to use {port}. On Linux, serial ports belong to the \"dialout\" group. To join it, run:\n  sudo usermod -aG dialout $USER\nthen log out and back in."
        )
    } else {
        format!("Access to {port} was refused. Another program may already be using it.")
    };
    AppError::new(ErrorCode::PermissionDenied, message)
}

fn open_port(config: &SerialConfig) -> Result<SerialStream, AppError> {
    use tokio_serial::{DataBits, FlowControl as Flow, Parity as Par, StopBits};
    let data_bits = match config.data_bits {
        5 => DataBits::Five,
        6 => DataBits::Six,
        7 => DataBits::Seven,
        _ => DataBits::Eight,
    };
    let parity = match config.parity {
        Parity::None => Par::None,
        Parity::Odd => Par::Odd,
        Parity::Even => Par::Even,
    };
    let stop_bits = if config.stop_bits == 2 {
        StopBits::Two
    } else {
        StopBits::One
    };
    let flow = match config.flow_control {
        FlowControl::None => Flow::None,
        FlowControl::Hardware => Flow::Hardware,
        FlowControl::Software => Flow::Software,
    };
    tokio_serial::new(&config.port, config.baud)
        .data_bits(data_bits)
        .parity(parity)
        .stop_bits(stop_bits)
        .flow_control(flow)
        .open_native_async()
        .map_err(|e| open_error(&config.port, &e))
}

/// What the task needs from a port, so tests can use a stand-in.
pub trait Port: AsyncRead + AsyncWrite + Unpin + Send + 'static {
    fn set_dtr(&mut self, on: bool) -> io::Result<()>;
    fn set_rts(&mut self, on: bool) -> io::Result<()>;
    fn hold_break(&mut self, on: bool) -> io::Result<()>;
}

impl Port for SerialStream {
    fn set_dtr(&mut self, on: bool) -> io::Result<()> {
        self.write_data_terminal_ready(on).map_err(io::Error::from)
    }

    fn set_rts(&mut self, on: bool) -> io::Result<()> {
        self.write_request_to_send(on).map_err(io::Error::from)
    }

    fn hold_break(&mut self, on: bool) -> io::Result<()> {
        if on {
            SerialPort::set_break(self)
        } else {
            SerialPort::clear_break(self)
        }
        .map_err(io::Error::from)
    }
}

enum Command {
    Write(Vec<u8>),
    Control(PortControl, oneshot::Sender<Result<(), AppError>>),
}

fn unplugged() -> AppError {
    AppError::new(
        ErrorCode::ConnectionLost,
        "The device was unplugged, or stopped answering.",
    )
}

/// What is sent for what was typed: the Enter key (CR) becomes the chosen line ending.
pub fn outgoing(data: &[u8], ending: LineEnding) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for &byte in data {
        match (byte, ending) {
            (b'\r', LineEnding::Lf) => out.push(b'\n'),
            (b'\r', LineEnding::Crlf) => out.extend_from_slice(b"\r\n"),
            _ => out.push(byte),
        }
    }
    out
}

/// What local echo shows for what was typed: the Enter key moves to the next line, and
/// backspace rubs out.
pub fn echoed(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for &byte in data {
        match byte {
            b'\r' => out.extend_from_slice(b"\r\n"),
            0x7f | 0x08 => out.extend_from_slice(b"\x08 \x08"),
            other => out.push(other),
        }
    }
    out
}

pub struct SerialBackend {
    commands: mpsc::Sender<Command>,
    events: mpsc::Sender<BackendEvent>,
    cancel: CancellationToken,
    local_echo: bool,
    line_ending: LineEnding,
}

async fn apply<P: Port>(port: &mut P, control: PortControl) -> Result<(), AppError> {
    let failed = |_| {
        AppError::new(
            ErrorCode::Unsupported,
            "This device or driver doesn't allow that.",
        )
    };
    match control {
        PortControl::Dtr(on) => port.set_dtr(on).map_err(failed),
        PortControl::Rts(on) => port.set_rts(on).map_err(failed),
        PortControl::Break => {
            port.hold_break(true).map_err(failed)?;
            tokio::time::sleep(BREAK_FOR).await;
            port.hold_break(false).map_err(failed)
        }
    }
}

async fn run<P: Port>(
    mut port: P,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<BackendEvent>,
    cancel: CancellationToken,
) {
    let mut buffer = vec![0u8; MAX_BACKEND_READ];
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            read = port.read(&mut buffer) => match read {
                // The end of the stream or an error means the device went.
                Ok(0) | Err(_) => {
                    let _ = events.send(BackendEvent::Failed(unplugged())).await;
                    return;
                }
                Ok(n) => {
                    if events.send(BackendEvent::Data(buffer[..n].to_vec())).await.is_err() {
                        return;
                    }
                }
            },
            command = commands.recv() => match command {
                None => return,
                Some(Command::Write(bytes)) => {
                    if port.write_all(&bytes).await.is_err() || port.flush().await.is_err() {
                        let _ = events.send(BackendEvent::Failed(unplugged())).await;
                        return;
                    }
                }
                Some(Command::Control(control, reply)) => {
                    let _ = reply.send(apply(&mut port, control).await);
                }
            },
        }
    }
}

#[async_trait]
impl TerminalBackend for SerialBackend {
    async fn write(&self, data: &[u8]) -> Result<(), AppError> {
        if self.local_echo {
            let _ = self.events.send(BackendEvent::Data(echoed(data))).await;
        }
        self.commands
            .send(Command::Write(outgoing(data, self.line_ending)))
            .await
            .map_err(|_| unplugged())
    }

    /// A serial line has no window size.
    async fn resize(&self, _size: crate::terminal::TermSize) -> Result<(), AppError> {
        Ok(())
    }

    async fn close(&self) -> Result<(), AppError> {
        self.cancel.cancel();
        Ok(())
    }

    async fn control(&self, control: PortControl) -> Result<(), AppError> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Control(control, reply))
            .await
            .map_err(|_| unplugged())?;
        answer.await.map_err(|_| unplugged())?
    }
}

/// Starts a session on an already open port.
pub fn start_session<P: Port>(
    registry: &SessionRegistry,
    port: P,
    config: &SerialConfig,
    sink: Arc<dyn OutputSink>,
) -> String {
    let (events, queue) = mpsc::channel(QUEUE_SIZE);
    let (commands, command_queue) = mpsc::channel(QUEUE_SIZE);
    let cancel = CancellationToken::new();
    let backend = Arc::new(SerialBackend {
        commands,
        events: events.clone(),
        cancel: cancel.clone(),
        local_echo: config.local_echo,
        line_ending: config.line_ending,
    });
    let pump = spawn_pump(queue, Arc::clone(&sink), PumpConfig::default());
    let id = registry.insert(Session::new(backend, pump));
    let _ = sink.send_status(TerminalStatus::Connected);
    tokio::spawn(run(port, command_queue, events, cancel));
    id
}

/// Opens a terminal on a serial port. If the port can't be opened, the session still exists and
/// says why through its status, so the pane can offer to try again.
pub async fn open_serial_terminal(
    registry: &SessionRegistry,
    config: SerialConfig,
    sink: Arc<dyn OutputSink>,
) -> Result<String, AppError> {
    let opening = config.clone();
    let opened = tokio::task::spawn_blocking(move || open_port(&opening))
        .await
        .map_err(|_| AppError::new(ErrorCode::Internal, "Couldn't open the serial port."))?;
    match opened {
        Ok(port) => Ok(start_session(registry, port, &config, sink)),
        Err(error) => {
            let (events, queue) = mpsc::channel(QUEUE_SIZE);
            let (commands, _) = mpsc::channel(1);
            let backend = Arc::new(SerialBackend {
                commands,
                events: events.clone(),
                cancel: CancellationToken::new(),
                local_echo: false,
                line_ending: config.line_ending,
            });
            let pump = spawn_pump(queue, sink, PumpConfig::default());
            let id = registry.insert(Session::new(backend, pump));
            let _ = events.send(BackendEvent::Failed(error)).await;
            Ok(id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::testing::RecordingSink;
    use std::pin::Pin;
    use std::sync::Mutex;
    use std::task::{Context, Poll};
    use tokio::io::{DuplexStream, ReadBuf};

    /// A stand-in port: the other end of a pipe, and a record of the control line changes.
    struct Fake {
        stream: DuplexStream,
        lines: Arc<Mutex<Vec<String>>>,
    }

    impl AsyncRead for Fake {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Pin::new(&mut self.stream).poll_read(cx, buf)
        }
    }

    impl AsyncWrite for Fake {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Pin::new(&mut self.stream).poll_write(cx, buf)
        }
        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.stream).poll_flush(cx)
        }
        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.stream).poll_shutdown(cx)
        }
    }

    impl Port for Fake {
        fn set_dtr(&mut self, on: bool) -> io::Result<()> {
            self.lines.lock().unwrap().push(format!("dtr {on}"));
            Ok(())
        }
        fn set_rts(&mut self, on: bool) -> io::Result<()> {
            self.lines.lock().unwrap().push(format!("rts {on}"));
            Ok(())
        }
        fn hold_break(&mut self, on: bool) -> io::Result<()> {
            self.lines.lock().unwrap().push(format!("break {on}"));
            Ok(())
        }
    }

    fn config(ending: LineEnding, echo: bool) -> SerialConfig {
        SerialConfig {
            port: "FAKE".into(),
            baud: 115_200,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: 1,
            flow_control: FlowControl::None,
            local_echo: echo,
            line_ending: ending,
        }
    }

    struct Rig {
        registry: SessionRegistry,
        sink: Arc<RecordingSink>,
        id: String,
        device: DuplexStream,
        lines: Arc<Mutex<Vec<String>>>,
    }

    fn rig(config: &SerialConfig) -> Rig {
        let (near, device) = tokio::io::duplex(4096);
        let lines = Arc::new(Mutex::new(Vec::new()));
        let registry = SessionRegistry::new();
        let sink = Arc::new(RecordingSink::default());
        let id = start_session(
            &registry,
            Fake {
                stream: near,
                lines: lines.clone(),
            },
            config,
            sink.clone(),
        );
        Rig {
            registry,
            sink,
            id,
            device,
            lines,
        }
    }

    async fn until(mut done: impl FnMut() -> bool) {
        for _ in 0..200 {
            if done() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("timed out");
    }

    fn received(sink: &RecordingSink) -> Vec<u8> {
        use crate::terminal::testing::Sent;
        sink.all()
            .into_iter()
            .filter_map(|s| match s {
                Sent::Data(d) => Some(d),
                Sent::Status(_) => None,
            })
            .flatten()
            .collect()
    }

    #[test]
    fn enter_becomes_the_chosen_line_ending() {
        assert_eq!(outgoing(b"ls\r", LineEnding::Cr), b"ls\r");
        assert_eq!(outgoing(b"ls\r", LineEnding::Lf), b"ls\n");
        assert_eq!(outgoing(b"ls\r", LineEnding::Crlf), b"ls\r\n");
        assert_eq!(outgoing(b"a\x03b", LineEnding::Crlf), b"a\x03b");
    }

    #[test]
    fn local_echo_moves_to_the_next_line_and_rubs_out() {
        assert_eq!(echoed(b"hi\r"), b"hi\r\n");
        assert_eq!(echoed(b"a\x7f"), b"a\x08 \x08");
    }

    #[test]
    fn ports_are_listed_in_natural_order() {
        let mut names = vec!["COM10", "COM2", "COM1"];
        names.sort_by_key(|n| natural(n));
        assert_eq!(names, ["COM1", "COM2", "COM10"]);
    }

    #[test]
    fn device_names_are_cleaned() {
        assert_eq!(plain("FTDI\x1b[31m\n Cable"), "FTDI[31m Cable");
        assert_eq!(plain(&"x".repeat(200)).len(), 80);
    }

    #[test]
    fn a_permission_error_explains_the_dialout_fix_on_linux() {
        let error = permission_denied("/dev/ttyUSB0");
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        if cfg!(target_os = "linux") {
            assert!(error.message.contains("sudo usermod -aG dialout $USER"));
            assert!(error.message.contains("log out"));
        } else {
            assert!(error.message.contains("/dev/ttyUSB0"));
        }
    }

    #[test]
    fn settings_come_from_a_saved_serial_session() {
        let saved = SavedSession::Serial {
            id: "s".into(),
            name: "Router".into(),
            folder: String::new(),
            port: " COM3 ".into(),
            baud: 9600,
            data_bits: 7,
            parity: Parity::Even,
            stop_bits: 2,
            flow_control: FlowControl::Hardware,
            local_echo: true,
            line_ending: LineEnding::Crlf,
        };
        let config = SerialConfig::from_saved(&saved).unwrap();
        assert_eq!(config.port, "COM3");
        assert_eq!(config.baud, 9600);
        assert!(config.local_echo);
        let other = SavedSession::Ftp {
            id: "f".into(),
            name: "x".into(),
            folder: String::new(),
            host: "h".into(),
            port: 21,
            user: String::new(),
            security: crate::sessions::FtpSecurity::Plain,
            trusted_cert_sha256: String::new(),
        };
        assert!(SerialConfig::from_saved(&other).is_err());
    }

    #[tokio::test]
    async fn what_the_device_sends_reaches_the_terminal() {
        let mut rig = rig(&config(LineEnding::Cr, false));
        rig.device.write_all(b"boot ok\r\n").await.unwrap();
        until(|| received(&rig.sink) == b"boot ok\r\n").await;
    }

    #[tokio::test]
    async fn what_is_typed_reaches_the_device_with_the_line_ending() {
        let mut rig = rig(&config(LineEnding::Crlf, false));
        rig.registry
            .get(&rig.id)
            .unwrap()
            .write(b"help\r")
            .await
            .unwrap();
        let mut got = vec![0u8; 6];
        rig.device.read_exact(&mut got).await.unwrap();
        assert_eq!(got, b"help\r\n");
        assert!(received(&rig.sink).is_empty(), "no echo unless asked for");
    }

    #[tokio::test]
    async fn local_echo_shows_what_was_typed() {
        let mut rig = rig(&config(LineEnding::Cr, true));
        rig.registry
            .get(&rig.id)
            .unwrap()
            .write(b"ab\r")
            .await
            .unwrap();
        let mut got = vec![0u8; 3];
        rig.device.read_exact(&mut got).await.unwrap();
        assert_eq!(got, b"ab\r");
        until(|| received(&rig.sink) == b"ab\r\n").await;
    }

    #[tokio::test]
    async fn the_control_lines_and_break_can_be_changed() {
        let rig = rig(&config(LineEnding::Cr, false));
        let session = rig.registry.get(&rig.id).unwrap();
        session.control(PortControl::Dtr(true)).await.unwrap();
        session.control(PortControl::Rts(false)).await.unwrap();
        session.control(PortControl::Break).await.unwrap();
        assert_eq!(
            *rig.lines.lock().unwrap(),
            ["dtr true", "rts false", "break true", "break false"]
        );
    }

    #[tokio::test]
    async fn unplugging_is_reported_so_the_pane_can_offer_to_reconnect() {
        let rig = rig(&config(LineEnding::Cr, false));
        drop(rig.device);
        until(|| {
            rig.sink.statuses().iter().any(|s| {
                matches!(s, TerminalStatus::Failed { error } if error.code == ErrorCode::ConnectionLost)
            })
        })
        .await;
    }

    #[tokio::test]
    async fn closing_stops_the_session() {
        let rig = rig(&config(LineEnding::Cr, false));
        rig.registry.close(&rig.id).await.unwrap();
        assert!(rig.registry.get(&rig.id).is_err());
    }

    #[tokio::test]
    async fn a_port_that_cannot_be_opened_gives_a_failed_status_not_a_crash() {
        let registry = SessionRegistry::new();
        let sink = Arc::new(RecordingSink::default());
        let mut missing = config(LineEnding::Cr, false);
        missing.port = if cfg!(windows) {
            "COM250"
        } else {
            "/dev/does-not-exist-tty"
        }
        .into();
        let id = open_serial_terminal(&registry, missing, sink.clone())
            .await
            .unwrap();
        assert!(registry.get(&id).is_ok());
        until(|| {
            sink.statuses()
                .iter()
                .any(|s| matches!(s, TerminalStatus::Failed { .. }))
        })
        .await;
    }
}
