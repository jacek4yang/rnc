use crate::encoding::Mode;
use std::{io, time::Duration};

pub const HELP: &str = "Usage: nc [options] host port\n       nc -l [options] [bind-address] port\n\nOptions:\n  -l                 Listen for one peer\n  -u                 UDP (one stdin read per datagram, up to 16 KiB)\n  -4 / -6            Use IPv4 / IPv6 exclusively\n  -v                 Diagnostics on stderr\n  -n                 Numeric addresses only\n  -w seconds         Connect and network idle timeout (positive, fractional OK)\n  -q seconds         Quit this long after stdin EOF (0 = immediately)\n  -N                 Half-close TCP on stdin EOF (already the default)\n  --encoding MODE    auto (default), utf-8, gbk/cp936, gb18030\n  --raw              Disable console encoding conversion\n  -h, --help         Show help\n  --version          Show version\n\nPipes/files are always raw. Auto defaults outgoing console text to UTF-8\nuntil incoming non-ASCII data identifies UTF-8 or GB18030. TCP waits for\nboth directions to finish; use -q or -w to bound that wait. UDP has no EOF;\nuse -q or -w to stop receiving. Ctrl+C exits with status 130.\n";

#[derive(Debug, Clone)]
pub struct Config {
    pub listen: bool,
    pub udp: bool,
    pub family: Option<bool>, // true = IPv6
    pub verbose: bool,
    pub numeric: bool,
    pub raw: bool,
    pub encoding: Mode,
    pub timeout: Option<Duration>,
    pub quit: Option<Duration>,
    pub host: String,
    pub port: u16,
}

impl Config {
    pub fn parse(args: impl IntoIterator<Item = String>) -> io::Result<Self> {
        let mut c = Self {
            listen: false,
            udp: false,
            family: None,
            verbose: false,
            numeric: false,
            raw: false,
            encoding: Mode::Auto,
            timeout: None,
            quit: None,
            host: String::new(),
            port: 0,
        };
        let mut args = args.into_iter();
        let mut positional = Vec::new();
        let mut options = true;
        while let Some(arg) = args.next() {
            if options && arg == "--" {
                options = false;
                continue;
            }
            if options && arg == "--raw" {
                c.raw = true;
                continue;
            }
            if options && (arg == "--encoding" || arg.starts_with("--encoding=")) {
                let value = arg
                    .strip_prefix("--encoding=")
                    .map(str::to_owned)
                    .or_else(|| args.next())
                    .ok_or_else(|| invalid("missing encoding"))?;
                c.encoding = Mode::parse(&value)?;
                continue;
            }
            if options && arg.starts_with('-') && arg.len() > 1 {
                let mut flags = arg[1..].char_indices().peekable();
                while let Some((offset, flag)) = flags.next() {
                    match flag {
                        'l' => c.listen = true,
                        'u' => c.udp = true,
                        'v' => c.verbose = true,
                        'n' => c.numeric = true,
                        'N' => {}
                        '4' | '6' => {
                            let family = flag == '6';
                            if c.family.is_some_and(|previous| previous != family) {
                                return Err(invalid("-4 and -6 are mutually exclusive"));
                            }
                            c.family = Some(family);
                        }
                        'w' | 'q' => {
                            let value = if flags.peek().is_some() {
                                arg[offset + 2..].to_owned()
                            } else {
                                args.next()
                                    .ok_or_else(|| invalid("missing timeout value"))?
                            };
                            let seconds: f64 =
                                value.parse().map_err(|_| invalid("invalid timeout"))?;
                            let duration = Duration::try_from_secs_f64(seconds)
                                .map_err(|_| invalid("timeout must be finite and nonnegative"))?;
                            if flag == 'w' {
                                if duration.is_zero() {
                                    return Err(invalid("-w must be positive"));
                                }
                                c.timeout = Some(duration);
                            } else {
                                c.quit = Some(duration);
                            }
                            break;
                        }
                        _ => return Err(invalid(format!("unknown option: {arg}"))),
                    }
                }
            } else {
                positional.push(arg);
            }
        }
        let port = match positional.as_slice() {
            [port] if c.listen => {
                c.host = if c.family == Some(true) {
                    "::"
                } else {
                    "0.0.0.0"
                }
                .into();
                port
            }
            [host, port] => {
                c.host = host
                    .trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_owned();
                port
            }
            _ => {
                return Err(invalid(
                    "expected host port, or -l [bind-address] port (see --help)",
                ));
            }
        };
        c.port = port
            .parse()
            .map_err(|_| invalid("port must be a number from 1 to 65535"))?;
        if c.port == 0 {
            return Err(invalid("port must be a number from 1 to 65535"));
        }
        Ok(c)
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(s: &str) -> io::Result<Config> {
        Config::parse(s.split_whitespace().map(str::to_owned))
    }
    #[test]
    fn syntax() {
        assert_eq!(parse("localhost 8080").unwrap().port, 8080);
        let c = parse("-6luv -w0.5 --encoding cp936 8080").unwrap();
        assert!(c.listen && c.udp && c.verbose);
        assert_eq!(c.host, "::");
        assert_eq!(c.encoding, Mode::Gbk);
        assert_eq!(parse("-l 127.0.0.1 1234").unwrap().host, "127.0.0.1");
        for bad in [
            "-4 -6 host 80",
            "host 0",
            "host 65536",
            "-w NaN host 80",
            "-w0 host 80",
            "--bad host 80",
            "-l",
            "host 80 extra",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }
}
