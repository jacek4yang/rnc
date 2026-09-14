use rnc::{
    cli::{Config, HELP},
    transport::{self, Event},
};
use std::sync::mpsc;

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return;
    }
    if args.iter().any(|a| a == "--version") {
        println!("nc {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let config = match Config::parse(args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("nc: {e}");
            std::process::exit(2);
        }
    };
    let (tx, rx) = mpsc::channel();
    let signal_tx = tx.clone();
    if let Err(e) = ctrlc::set_handler(move || {
        let _ = signal_tx.send(Event::Interrupted);
    }) {
        eprintln!("nc: cannot install Ctrl+C handler: {e}");
        std::process::exit(1);
    }
    let code = match transport::run(config, tx, rx) {
        Ok(false) => 0,
        Ok(true) => 130,
        Err(e) => {
            eprintln!("nc: {e}");
            1
        }
    };
    // A blocked pipe/console read cannot be joined portably. Output writes are
    // explicitly flushed. Process exit lets the OS cancel pending I/O and close
    // all handles, without racing cancellation against a new blocking read.
    std::process::exit(code);
}
