//! The only native/unsafe boundary. Redirected handles use ordinary byte I/O.
#[cfg(windows)]
use crate::encoding::ConsoleDecoder;
use crate::encoding::Selection;
#[cfg(not(windows))]
use std::io::Write;
use std::io::{self, Read};

pub struct Input {
    #[cfg(windows)]
    console: Option<native::ConsoleInput>,
    #[cfg(windows)]
    selection: Selection,
}
impl Input {
    pub fn new(raw: bool, selection: Selection) -> Self {
        #[cfg(windows)]
        {
            Self {
                console: (!raw).then(native::ConsoleInput::new).flatten(),
                selection,
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (raw, selection);
            Self {}
        }
    }
}
impl Read for Input {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        #[cfg(windows)]
        if let Some(console) = &mut self.console {
            return console.read(buffer, &self.selection);
        }
        #[cfg(windows)]
        {
            native::read_bytes(buffer)
        }
        #[cfg(not(windows))]
        {
            io::stdin().read(buffer)
        }
    }
}

pub struct Output {
    #[cfg(windows)]
    console: Option<(native::ConsoleOutput, ConsoleDecoder)>,
}
impl Output {
    pub fn new(raw: bool, selection: Selection) -> Self {
        #[cfg(windows)]
        {
            Self {
                console: if raw {
                    None
                } else {
                    native::ConsoleOutput::new().map(|c| (c, ConsoleDecoder::new(selection)))
                },
            }
        }
        #[cfg(not(windows))]
        {
            let _ = (raw, selection);
            Self {}
        }
    }
    pub fn push(&mut self, bytes: &[u8], last: bool) -> io::Result<()> {
        #[cfg(windows)]
        if let Some((console, decoder)) = &mut self.console {
            return decoder.push(bytes, last, |text| console.write(text));
        }
        let _ = last;
        #[cfg(windows)]
        {
            native::write_bytes(bytes)
        }
        #[cfg(not(windows))]
        {
            let mut out = io::stdout().lock();
            out.write_all(bytes)?;
            out.flush()
        }
    }
}

#[cfg(windows)]
pub mod native {
    use super::*;
    use windows_sys::Win32::{
        Foundation::{ERROR_BROKEN_PIPE, ERROR_HANDLE_EOF, HANDLE, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{ReadFile, WriteFile},
        System::Console::{
            GetConsoleMode, GetStdHandle, ReadConsoleW, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
            WriteConsoleW,
        },
    };

    pub fn read_bytes(buffer: &mut [u8]) -> io::Result<usize> {
        let mut read = 0;
        // SAFETY: the borrowed standard handle is used synchronously; buffer is writable
        // for the requested length. No OVERLAPPED structure is used.
        let ok = unsafe {
            ReadFile(
                GetStdHandle(STD_INPUT_HANDLE),
                buffer.as_mut_ptr(),
                buffer.len().min(u32::MAX as usize) as u32,
                &mut read,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            let e = io::Error::last_os_error();
            if matches!(e.raw_os_error(), Some(code) if code == ERROR_BROKEN_PIPE as i32 || code == ERROR_HANDLE_EOF as i32)
            {
                return Ok(0);
            }
            return Err(e);
        }
        Ok(read as usize)
    }

    pub fn write_bytes(mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            let mut written = 0;
            // SAFETY: the standard output handle is borrowed, bytes remains readable
            // throughout this synchronous call, and written points to live storage.
            let ok = unsafe {
                WriteFile(
                    GetStdHandle(STD_OUTPUT_HANDLE),
                    bytes.as_ptr(),
                    bytes.len().min(u32::MAX as usize) as u32,
                    &mut written,
                    std::ptr::null_mut(),
                )
            };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            if written == 0 {
                return Err(io::ErrorKind::WriteZero.into());
            }
            bytes = &bytes[written as usize..];
        }
        Ok(())
    }

    fn console_handle(id: u32) -> Option<HANDLE> {
        // SAFETY: these APIs accept the standard handle IDs; mode points to live storage.
        unsafe {
            let handle = GetStdHandle(id);
            let mut mode = 0;
            if handle.is_null()
                || handle == INVALID_HANDLE_VALUE
                || GetConsoleMode(handle, &mut mode) == 0
            {
                None
            } else {
                Some(handle)
            }
        }
    }
    pub struct ConsoleInput {
        handle: HANDLE,
        wide: Box<[u16; 4096]>,
        text: String,
        pending: Vec<u8>,
        offset: usize,
        high: Option<u16>,
        carriage_return: bool,
    }
    impl ConsoleInput {
        pub fn new() -> Option<Self> {
            console_handle(STD_INPUT_HANDLE).map(|handle| Self {
                handle,
                wide: Box::new([0; 4096]),
                text: String::with_capacity(8192),
                pending: Vec::with_capacity(16384),
                offset: 0,
                high: None,
                carriage_return: false,
            })
        }
        pub fn read(&mut self, buffer: &mut [u8], selection: &Selection) -> io::Result<usize> {
            if buffer.is_empty() {
                return Ok(0);
            }
            while self.offset == self.pending.len() {
                self.pending.clear();
                self.offset = 0;
                self.text.clear();
                let start = if let Some(high) = self.high.take() {
                    self.wide[0] = high;
                    1
                } else {
                    0
                };
                let mut count = 0;
                // SAFETY: handle is a console input handle and the buffer has the requested writable capacity.
                let ok = unsafe {
                    ReadConsoleW(
                        self.handle,
                        self.wide[start..].as_mut_ptr().cast(),
                        (self.wide.len() - start) as u32,
                        &mut count,
                        std::ptr::null(),
                    )
                };
                if ok == 0 {
                    return Err(io::Error::last_os_error());
                }
                let mut end = start + count as usize;
                // Cooked Windows EOF is Ctrl+Z followed by Enter, at the start of a line.
                if count == 0 || (start == 0 && self.wide[0] == 0x1a) {
                    return Ok(0);
                }
                if (0xd800..=0xdbff).contains(&self.wide[end - 1]) {
                    self.high = Some(self.wide[end - 1]);
                    end -= 1;
                }
                for ch in char::decode_utf16(self.wide[..end].iter().copied()) {
                    let ch = ch.unwrap_or(char::REPLACEMENT_CHARACTER);
                    // Normalize console CRLF even when split between reads.
                    if self.carriage_return && ch != '\n' {
                        self.text.push('\r');
                    }
                    self.carriage_return = ch == '\r';
                    if !self.carriage_return {
                        self.text.push(ch);
                    }
                }
                // The receive worker may identify the encoding while ReadConsoleW
                // is blocked. Sample the selection only after the user submits input.
                let (bytes, _, errors) = selection.mode().codec().encode(&self.text);
                if errors {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "console input is not representable in selected encoding; use utf-8 or gb18030",
                    ));
                }
                self.pending.extend_from_slice(&bytes);
            }
            let n = buffer.len().min(self.pending.len() - self.offset);
            buffer[..n].copy_from_slice(&self.pending[self.offset..self.offset + n]);
            self.offset += n;
            Ok(n)
        }
    }
    pub struct ConsoleOutput {
        handle: HANDLE,
    }
    impl ConsoleOutput {
        pub fn new() -> Option<Self> {
            console_handle(STD_OUTPUT_HANDLE).map(|handle| Self { handle })
        }
        pub fn write(&mut self, mut text: &[u16]) -> io::Result<()> {
            while !text.is_empty() {
                let mut written = 0;
                // SAFETY: handle is a console output handle; text is a valid readable UTF-16 slice.
                let ok = unsafe {
                    WriteConsoleW(
                        self.handle,
                        text.as_ptr().cast(),
                        text.len().min(8192) as u32,
                        &mut written,
                        std::ptr::null(),
                    )
                };
                if ok == 0 {
                    return Err(io::Error::last_os_error());
                }
                if written == 0 {
                    return Err(io::ErrorKind::WriteZero.into());
                }
                text = &text[written as usize..];
            }
            Ok(())
        }
    }
}
