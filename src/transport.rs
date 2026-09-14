//! Blocking byte transport. No decoding, line processing, or terminal APIs here.
use crate::{
    cli::Config,
    encoding::Selection,
    terminal::{Input, Output},
};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    io::{self, Read, Write},
    net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};

pub const TCP_BUFFER: usize = 256 * 1024;
pub const UDP_PAYLOAD: usize = 16 * 1024;

pub enum Event {
    Connected(TcpStream),
    InputDone,
    OutputDone,
    Error(io::Error),
    Interrupted,
}

#[derive(Default)]
struct Connection(Option<TcpStream>);
impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(stream) = &self.0 {
            // Issue an orderly shutdown before process exit cancels blocked I/O.
            let _ = stream.shutdown(Shutdown::Both);
        }
    }
}
#[derive(Clone)]
struct Activity {
    enabled: bool,
    epoch: Instant,
    millis: Arc<AtomicU64>,
}
impl Activity {
    fn new(enabled: bool) -> Self {
        Self {
            enabled,
            epoch: Instant::now(),
            millis: Arc::new(AtomicU64::new(0)),
        }
    }
    fn touch(&self) {
        if !self.enabled {
            return;
        }
        self.millis
            .fetch_max(self.epoch.elapsed().as_millis() as u64, Ordering::Relaxed);
    }
    fn idle(&self) -> Duration {
        self.epoch
            .elapsed()
            .saturating_sub(Duration::from_millis(self.millis.load(Ordering::Relaxed)))
    }
}

pub fn run(config: Config, tx: Sender<Event>, rx: Receiver<Event>) -> io::Result<bool> {
    let activity = Activity::new(config.timeout.is_some());
    let worker_activity = activity.clone();
    let worker_config = config.clone();
    let worker_tx = tx.clone();
    thread::Builder::new()
        .name("nc-connect".into())
        .spawn(move || {
            if let Err(e) = start(worker_config, worker_tx.clone(), worker_activity) {
                let _ = worker_tx.send(Event::Error(e));
            }
        })?;
    let mut input_done = false;
    let mut connection = Connection::default();
    let mut output_done = false;
    let mut quit_at = None;
    loop {
        let event = if config.timeout.is_none() && quit_at.is_none() {
            rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
        } else {
            rx.recv_timeout(Duration::from_millis(20))
        };
        match event {
            Ok(Event::Connected(stream)) => connection.0 = Some(stream),
            Ok(Event::Interrupted) => return Ok(true),
            Ok(Event::Error(e)) => return Err(e),
            Ok(Event::InputDone) => {
                input_done = true;
                quit_at = config.quit.map(|d| (Instant::now(), d));
            }
            Ok(Event::OutputDone) => output_done = true,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(io::Error::other("workers disconnected"));
            }
        }
        if input_done && output_done {
            return Ok(false);
        }
        if quit_at.is_some_and(|(start, duration)| start.elapsed() >= duration) {
            return Ok(false);
        }
        if config
            .timeout
            .is_some_and(|timeout| activity.idle() >= timeout)
        {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "network idle/connect timeout",
            ));
        }
    }
}

fn addresses(c: &Config) -> io::Result<Vec<SocketAddr>> {
    let addresses: Vec<_> = if c.numeric {
        vec![SocketAddr::new(
            c.host.parse::<IpAddr>().map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "-n requires a numeric IP address",
                )
            })?,
            c.port,
        )]
    } else {
        (c.host.as_str(), c.port).to_socket_addrs()?.collect()
    };
    let addresses: Vec<_> = addresses
        .into_iter()
        .filter(|a| c.family.is_none_or(|v6| a.is_ipv6() == v6))
        .collect();
    if addresses.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "no address matches selected IP family",
        ));
    }
    Ok(addresses)
}
fn bind_socket(address: SocketAddr, udp: bool) -> io::Result<Socket> {
    let socket = Socket::new(
        Domain::for_address(address),
        if udp { Type::DGRAM } else { Type::STREAM },
        Some(if udp { Protocol::UDP } else { Protocol::TCP }),
    )?;
    if address.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    // Do not use SO_REUSEADDR on Windows: it permits stealing an active listener.
    #[cfg(not(windows))]
    socket.set_reuse_address(true)?;
    socket.bind(&address.into())?;
    Ok(socket)
}
fn first_success<T>(
    addresses: &[SocketAddr],
    mut f: impl FnMut(SocketAddr) -> io::Result<T>,
) -> io::Result<T> {
    let mut error = io::Error::other("no usable address");
    for &address in addresses {
        match f(address) {
            Ok(value) => return Ok(value),
            Err(e) => error = e,
        }
    }
    Err(error)
}
fn start(c: Config, tx: Sender<Event>, activity: Activity) -> io::Result<()> {
    let addresses = addresses(&c)?;
    if c.udp {
        return start_udp(c, tx, activity, &addresses);
    }
    let stream = if c.listen {
        let listener: TcpListener = first_success(&addresses, |address| {
            let socket = bind_socket(address, false)?;
            socket.listen(1)?;
            Ok(socket)
        })?
        .into();
        if c.verbose {
            eprintln!("nc: listening on {} (TCP)", listener.local_addr()?);
        }
        let (stream, _) = listener.accept()?;
        stream
    } else {
        first_success(&addresses, |address| match c.timeout {
            Some(timeout) => TcpStream::connect_timeout(&address, timeout),
            None => TcpStream::connect(address),
        })?
    };
    stream.set_nodelay(true)?;
    tx.send(Event::Connected(stream.try_clone()?))
        .map_err(io::Error::other)?;
    activity.touch();
    if c.verbose {
        eprintln!("nc: connected to {} (TCP)", stream.peer_addr()?);
    }
    let mut writer = stream.try_clone()?;
    let selection = Selection::new(c.encoding);
    let input_selection = selection.clone();
    let input_tx = tx.clone();
    let input_activity = activity.clone();
    let raw = c.raw;
    thread::Builder::new()
        .name("nc-stdin".into())
        .spawn(move || {
            let result = (|| {
                let mut input = Input::new(raw, input_selection);
                let mut buffer = vec![0; TCP_BUFFER];
                loop {
                    let n = read_retry(&mut input, &mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    let mut bytes = &buffer[..n];
                    while !bytes.is_empty() {
                        match writer.write(bytes) {
                            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
                            Ok(written) => {
                                input_activity.touch();
                                bytes = &bytes[written..];
                            }
                            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                            Err(e) => return Err(e),
                        }
                    }
                }
                match writer.shutdown(Shutdown::Write) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == io::ErrorKind::NotConnected => Ok(()),
                    Err(e) => Err(e),
                }
            })();
            report(&input_tx, result, Event::InputDone);
        })?;
    let mut reader = stream;
    let mut output = Output::new(c.raw, selection);
    let mut buffer = vec![0; TCP_BUFFER];
    loop {
        let n = read_retry(&mut reader, &mut buffer)?;
        if n == 0 {
            output.push(&[], true)?;
            break;
        }
        activity.touch();
        output.push(&buffer[..n], false)?;
    }
    tx.send(Event::OutputDone).map_err(io::Error::other)
}
fn start_udp(
    c: Config,
    tx: Sender<Event>,
    activity: Activity,
    addresses: &[SocketAddr],
) -> io::Result<()> {
    let socket: UdpSocket = if c.listen {
        first_success(addresses, |address| bind_socket(address, true))?.into()
    } else {
        first_success(addresses, |address| {
            let local = if address.is_ipv6() {
                "[::]:0"
            } else {
                "0.0.0.0:0"
            };
            let socket = UdpSocket::bind(local)?;
            socket.connect(address)?;
            Ok(socket)
        })?
    };
    let selection = Selection::new(c.encoding);
    let mut output = Output::new(c.raw, selection.clone());
    let mut buffer = vec![0; 65536];
    if c.listen {
        if c.verbose {
            eprintln!(
                "nc: listening on {} (UDP; first peer wins)",
                socket.local_addr()?
            );
        }
        let (n, peer) = socket.recv_from(&mut buffer)?;
        socket.connect(peer)?;
        activity.touch();
        output.push(&buffer[..n], false)?;
    }
    if c.verbose {
        eprintln!(
            "nc: peer {} (UDP; delivery is not guaranteed)",
            socket.peer_addr()?
        );
    }
    activity.touch();
    let writer = socket.try_clone()?;
    let input_tx = tx.clone();
    let input_activity = activity.clone();
    thread::Builder::new()
        .name("nc-stdin".into())
        .spawn(move || {
            let result = (|| {
                let mut input = Input::new(c.raw, selection);
                let mut buffer = vec![0; UDP_PAYLOAD];
                loop {
                    let n = read_retry(&mut input, &mut buffer)?;
                    if n == 0 {
                        return Ok(());
                    }
                    if writer.send(&buffer[..n])? != n {
                        return Err(io::ErrorKind::WriteZero.into());
                    }
                    input_activity.touch();
                }
            })();
            report(&input_tx, result, Event::InputDone);
        })?;
    loop {
        // A zero-length UDP datagram is valid data, not EOF.
        let n = socket.recv(&mut buffer)?;
        activity.touch();
        output.push(&buffer[..n], false)?;
    }
}
fn read_retry(reader: &mut impl Read, buffer: &mut [u8]) -> io::Result<usize> {
    loop {
        match reader.read(buffer) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            result => return result,
        }
    }
}
fn report(tx: &Sender<Event>, result: io::Result<()>, done: Event) {
    let _ = tx.send(match result {
        Ok(()) => done,
        Err(e) => Event::Error(e),
    });
}
