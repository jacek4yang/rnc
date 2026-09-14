use std::{
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream, UdpSocket},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

struct Process(Child);
impl Process {
    fn spawn(args: &[&str]) -> Self {
        Self(
            Command::new(env!("CARGO_BIN_EXE_nc"))
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
    }
    fn wait(&mut self, success: bool) {
        let start = Instant::now();
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                let mut err = String::new();
                self.0
                    .stderr
                    .take()
                    .unwrap()
                    .read_to_string(&mut err)
                    .unwrap();
                assert_eq!(status.success(), success, "status {status}: {err}");
                return;
            }
            assert!(start.elapsed() < Duration::from_secs(15), "nc hung");
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn socket_options(stream: &TcpStream) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
}
fn payload(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i % 256) as u8).collect()
}
fn accept(listener: &TcpListener) -> TcpStream {
    listener.set_nonblocking(true).unwrap();
    let start = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                socket_options(&stream);
                return stream;
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => panic!("{e}"),
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "accept timed out"
        );
        thread::sleep(Duration::from_millis(10));
    }
}
fn client_roundtrip(host: &str, family: &str, size: usize, extra: &[&str]) {
    let listener = TcpListener::bind((host, 0)).unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let mut args = vec![family, "-w", "10", host, &port];
    args.extend_from_slice(extra);
    let mut child = Process::spawn(&args);
    let mut input = child.0.stdin.take().unwrap();
    let expected = payload(size);
    let input_data = expected.clone();
    let writer = thread::spawn(move || {
        input.write_all(&input_data).unwrap();
        drop(input);
    });
    let mut output = child.0.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        output.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let mut socket = accept(&listener);
    let mut received = Vec::new();
    // Reply only AFTER observing FIN: catches premature full close on stdin EOF.
    if let Err(error) = socket.read_to_end(&mut received) {
        let writer_finished = writer.is_finished();
        let status = child.0.try_wait().unwrap();
        let _ = child.0.kill();
        let _ = child.0.wait();
        let mut stderr = String::new();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        panic!(
            "receive failed: {error}; got {}/{} bytes; stdin writer done={writer_finished}; status={status:?}; stderr={stderr}",
            received.len(),
            expected.len()
        );
    }
    assert_eq!(received, expected);
    socket.write_all(&received).unwrap();
    socket.shutdown(Shutdown::Write).unwrap();
    writer.join().unwrap();
    child.wait(true);
    assert_eq!(reader.join().unwrap(), expected);
}
#[test]
fn tcp_ipv4_binary_pipes() {
    client_roundtrip("127.0.0.1", "-4", 65536, &[]);
}
#[test]
fn tcp_ipv6_binary_pipes() {
    client_roundtrip("::1", "-6", 65536, &[]);
}
#[test]
fn encoding_never_changes_piped_bytes() {
    for mode in ["utf-8", "gbk", "gb18030", "auto"] {
        client_roundtrip("127.0.0.1", "-4", 8192, &["--encoding", mode]);
    }
}
#[test]
fn raw_large_transfer() {
    client_roundtrip("127.0.0.1", "-4", 16 * 1024 * 1024, &["--raw"]);
}
#[test]
fn listener_and_reverse_half_close() {
    for host in ["127.0.0.1", "::1"] {
        let reservation = TcpListener::bind((host, 0)).unwrap();
        let port = reservation.local_addr().unwrap().port();
        drop(reservation);
        let mut child = Process::spawn(&["-l", "-w", "10", host, &port.to_string()]);
        let start = Instant::now();
        let mut socket = loop {
            if let Ok(s) = TcpStream::connect((host, port)) {
                break s;
            }
            assert!(start.elapsed() < Duration::from_secs(10));
            thread::sleep(Duration::from_millis(10));
        };
        socket_options(&socket);
        socket.write_all(b"request").unwrap();
        socket.shutdown(Shutdown::Write).unwrap();
        let mut output = child.0.stdout.take().unwrap();
        let mut request = [0; 7];
        output.read_exact(&mut request).unwrap();
        assert_eq!(&request, b"request");
        // Peer FIN must leave the local sending direction operational.
        thread::sleep(Duration::from_millis(50));
        assert!(child.0.try_wait().unwrap().is_none());
        child
            .0
            .stdin
            .take()
            .unwrap()
            .write_all(b"response after FIN")
            .unwrap();
        let mut bytes = Vec::new();
        socket.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"response after FIN");
        child.wait(true);
    }
}
#[test]
fn simultaneous_full_duplex() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut child = Process::spawn(&[
        "-w10",
        "localhost",
        &listener.local_addr().unwrap().port().to_string(),
    ]);
    let data = payload(4 * 1024 * 1024);
    let outgoing = data.clone();
    let mut input = child.0.stdin.take().unwrap();
    let writer = thread::spawn(move || input.write_all(&outgoing).unwrap());
    let mut output = child.0.stdout.take().unwrap();
    let reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        output.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let mut socket = accept(&listener);
    let mut send_socket = socket.try_clone().unwrap();
    let outgoing = data.clone();
    let peer_writer = thread::spawn(move || {
        send_socket.write_all(&outgoing).unwrap();
        send_socket.shutdown(Shutdown::Write).unwrap();
    });
    let mut received = Vec::new();
    socket.read_to_end(&mut received).unwrap();
    writer.join().unwrap();
    peer_writer.join().unwrap();
    child.wait(true);
    assert_eq!(received, data);
    assert_eq!(reader.join().unwrap(), data);
}
#[test]
fn udp_client_and_listener() {
    for host in ["127.0.0.1", "::1"] {
        let server = UdpSocket::bind((host, 0)).unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut child = Process::spawn(&[
            "-u",
            "-w5",
            "-q0.5",
            host,
            &server.local_addr().unwrap().port().to_string(),
        ]);
        let data = payload(256);
        child.0.stdin.take().unwrap().write_all(&data).unwrap();
        let mut buffer = [0; 65536];
        let (n, peer) = server.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..n], data);
        server.send_to(&[], peer).unwrap();
        server.send_to(&data, peer).unwrap();
        let mut output = Vec::new();
        child
            .0
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut output)
            .unwrap();
        child.wait(true);
        assert_eq!(output, data);

        let reservation = UdpSocket::bind((host, 0)).unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);
        let mut child =
            Process::spawn(&["-ulv", "-w5", "-q0.5", host, &address.port().to_string()]);
        // Read the readiness diagnostic, without probing and accidentally selecting a peer.
        let mut err = std::io::BufReader::new(child.0.stderr.take().unwrap());
        let mut line = String::new();
        std::io::BufRead::read_line(&mut err, &mut line).unwrap();
        assert!(line.contains("listening"));
        child.0.stderr = Some(err.into_inner());
        server.send_to(&[], address).unwrap();
        server.send_to(&data, address).unwrap();
        let mut out = child.0.stdout.take().unwrap();
        let mut received = vec![0; data.len()];
        out.read_exact(&mut received).unwrap();
        assert_eq!(received, data);
        child.0.stdin.take().unwrap().write_all(&data).unwrap();
        let (n, _) = server.recv_from(&mut buffer).unwrap();
        assert_eq!(&buffer[..n], data);
        child.wait(true);
    }
}
#[test]
fn bounded_waits_and_errors() {
    let mut child = Process::spawn(&["-l", "127.0.0.1", "0"]);
    child.wait(false);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut child = Process::spawn(&[
        "-w0.1",
        "127.0.0.1",
        &listener.local_addr().unwrap().port().to_string(),
    ]);
    let _socket = accept(&listener);
    child.wait(false);
    let mut child = Process::spawn(&[
        "-q0",
        "127.0.0.1",
        &listener.local_addr().unwrap().port().to_string(),
    ]);
    let _socket = accept(&listener);
    drop(child.0.stdin.take());
    child.wait(true);
}

#[test]
fn large_udp_datagram_is_not_truncated() {
    for host in ["127.0.0.1", "::1"] {
        let server = UdpSocket::bind((host, 0)).unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut child = Process::spawn(&[
            "-u",
            "-q0.5",
            "-w5",
            host,
            &server.local_addr().unwrap().port().to_string(),
        ]);
        child.0.stdin.take().unwrap().write_all(b"hello").unwrap();
        let mut buffer = [0; 64];
        let (_, peer) = server.recv_from(&mut buffer).unwrap();
        let bytes = payload(60000);
        server.send_to(&bytes, peer).unwrap();
        let mut output = Vec::new();
        child
            .0
            .stdout
            .take()
            .unwrap()
            .read_to_end(&mut output)
            .unwrap();
        child.wait(true);
        assert_eq!(output, bytes);
    }
}

#[test]
fn address_validation_and_bind_conflict() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    for args in [
        vec!["-l", "127.0.0.1", &port],
        vec!["-n", "localhost", &port],
        vec!["-4", "::1", &port],
    ] {
        let mut child = Process::spawn(&args);
        child.wait(false);
    }
}
