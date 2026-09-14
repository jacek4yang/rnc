//! Encoding lives at the console boundary, never in socket transport.
use encoding_rs::{CoderResult, Decoder, Encoding, GB18030, GBK, UTF_8};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Utf8,
    Gbk,
    Gb18030,
}
impl Mode {
    pub fn parse(s: &str) -> io::Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "utf-8" | "utf8" => Ok(Self::Utf8),
            "gbk" | "cp936" => Ok(Self::Gbk),
            "gb18030" => Ok(Self::Gb18030),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "encoding must be auto, utf-8, gbk/cp936, or gb18030",
            )),
        }
    }
    pub fn codec(self) -> &'static Encoding {
        match self {
            Self::Auto | Self::Utf8 => UTF_8,
            Self::Gbk => GBK,
            Self::Gb18030 => GB18030,
        }
    }
}

#[derive(Clone)]
pub struct Selection(Arc<AtomicU8>);
impl Selection {
    pub fn new(mode: Mode) -> Self {
        Self(Arc::new(AtomicU8::new(mode as u8)))
    }
    pub fn mode(&self) -> Mode {
        match self.0.load(Ordering::Relaxed) {
            1 => Mode::Utf8,
            2 => Mode::Gbk,
            3 => Mode::Gb18030,
            _ => Mode::Auto,
        }
    }
    fn set(&self, mode: Mode) {
        self.0.store(mode as u8, Ordering::Relaxed);
    }
}

pub struct ConsoleDecoder {
    selection: Selection,
    decoder: Option<Decoder>,
    pending: Vec<u8>,
    output: Box<[u16; 8192]>,
}
impl ConsoleDecoder {
    pub fn new(selection: Selection) -> Self {
        let mode = selection.mode();
        Self {
            selection,
            decoder: (mode != Mode::Auto).then(|| mode.codec().new_decoder_without_bom_handling()),
            pending: Vec::new(),
            output: Box::new([0; 8192]),
        }
    }
    pub fn push(
        &mut self,
        bytes: &[u8],
        last: bool,
        mut emit: impl FnMut(&[u16]) -> io::Result<()>,
    ) -> io::Result<()> {
        if self.decoder.is_some() {
            return self.decode(bytes, last, &mut emit);
        }
        if self.pending.is_empty() && bytes.is_ascii() {
            // ASCII carries no evidence about the encoding. Do not commit.
            for chunk in bytes.chunks(self.output.len()) {
                for (dst, src) in self.output.iter_mut().zip(chunk) {
                    *dst = u16::from(*src);
                }
                emit(&self.output[..chunk.len()])?;
            }
            return Ok(());
        }
        self.pending.extend_from_slice(bytes);
        let mode = match std::str::from_utf8(&self.pending) {
            Ok(_) => Mode::Utf8,
            Err(e) if e.error_len().is_some() => Mode::Gb18030,
            Err(e)
                if self.pending[..e.valid_up_to()]
                    .iter()
                    .any(|b| !b.is_ascii()) =>
            {
                Mode::Utf8
            }
            Err(_) if !last => return Ok(()),
            Err(_) => Mode::Gb18030,
        };
        self.selection.set(mode);
        self.decoder = Some(mode.codec().new_decoder_without_bom_handling());
        let pending = std::mem::take(&mut self.pending);
        self.decode(&pending, last, &mut emit)
    }
    fn decode(
        &mut self,
        mut bytes: &[u8],
        last: bool,
        emit: &mut impl FnMut(&[u16]) -> io::Result<()>,
    ) -> io::Result<()> {
        let decoder = self.decoder.as_mut().expect("encoding selected");
        loop {
            let (result, read, written, _) =
                decoder.decode_to_utf16(bytes, &mut *self.output, last);
            emit(&self.output[..written])?;
            bytes = &bytes[read..];
            if result == CoderResult::InputEmpty {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn decode(mode: Mode, bytes: &[u8], split: usize) -> String {
        let mut d = ConsoleDecoder::new(Selection::new(mode));
        let mut result = Vec::new();
        for chunk in bytes.chunks(split) {
            d.push(chunk, false, |s| {
                result.extend_from_slice(s);
                Ok(())
            })
            .unwrap();
        }
        d.push(&[], true, |s| {
            result.extend_from_slice(s);
            Ok(())
        })
        .unwrap();
        String::from_utf16(&result).unwrap()
    }
    #[test]
    fn every_split_all_encodings() {
        for (mode, text) in [
            (Mode::Utf8, "你好世界🙂\r\n"),
            (Mode::Gbk, "你好世界\r\n"),
            (Mode::Gb18030, "你好世界𠀀🙂\r\n"),
        ] {
            let (bytes, _, errors) = mode.codec().encode(text);
            assert!(!errors);
            for n in 1..=bytes.len() {
                assert_eq!(decode(mode, &bytes, n), text);
                assert_eq!(decode(Mode::Auto, &bytes, n), text);
            }
        }
    }
    #[test]
    fn ascii_does_not_lock() {
        let selection = Selection::new(Mode::Auto);
        let mut d = ConsoleDecoder::new(selection.clone());
        d.push(b"banner > ", false, |_| Ok(())).unwrap();
        assert_eq!(selection.mode(), Mode::Auto);
        d.push(&[0xc4], false, |_| Ok(())).unwrap();
        assert_eq!(selection.mode(), Mode::Auto);
        d.push(&[0xe3], false, |_| Ok(())).unwrap();
        assert_eq!(selection.mode(), Mode::Gb18030);
    }
    #[test]
    fn malformed_and_truncated() {
        assert_eq!(decode(Mode::Utf8, &[0xe4, 0xbd], 1), "�");
        assert_eq!(decode(Mode::Gbk, &[0xc4], 1), "�");
        assert_eq!(decode(Mode::Gb18030, &[0x81, 0x30, 0x81], 1), "�");
    }
}
