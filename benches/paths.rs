//! Measures decoding only, excluding console rendering. No benchmark runtime dependency.
use rnc::encoding::{ConsoleDecoder, Mode, Selection};
use std::{hint::black_box, time::Instant};

fn main() {
    for (label, mode, text) in [
        (
            "auto ASCII (undecided)",
            Mode::Auto,
            "ASCII banner > 0123456789\n",
        ),
        ("UTF-8 Chinese", Mode::Utf8, "你好世界，测试文本。\n"),
        ("GBK Chinese", Mode::Gbk, "你好世界，测试文本。\n"),
        ("GB18030 supplementary", Mode::Gb18030, "你好世界𠀀🙂\n"),
        ("auto -> GB18030", Mode::Auto, "你好世界𠀀🙂\n"),
    ] {
        let codec = if mode == Mode::Auto && !text.is_ascii() {
            Mode::Gb18030.codec()
        } else {
            mode.codec()
        };
        let (encoded, _, errors) = codec.encode(text);
        assert!(!errors);
        let buffer = encoded.repeat((65536 / encoded.len()).max(1));
        let rounds = 2048;
        let mut decoder = ConsoleDecoder::new(Selection::new(mode));
        let start = Instant::now();
        let mut units = 0usize;
        for _ in 0..rounds {
            decoder
                .push(black_box(&buffer), false, |text| {
                    units += black_box(text).len();
                    Ok(())
                })
                .unwrap();
        }
        decoder.push(&[], true, |_| Ok(())).unwrap();
        let elapsed = start.elapsed();
        black_box(units);
        println!(
            "{label}: {:.1} MiB/s ({:.3}s, {} input bytes)",
            (buffer.len() * rounds) as f64 / 1048576.0 / elapsed.as_secs_f64(),
            elapsed.as_secs_f64(),
            buffer.len() * rounds
        );
    }
}
