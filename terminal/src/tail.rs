//! Follows a growing log file by polling its size. Survives truncation and rotation.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

/// Never read more than this per poll; if the file jumped further, skip ahead.
const MAX_READ: u64 = 8 << 20;

pub struct Tail {
    path: PathBuf,
    pos: u64,
    ino: u64,
    buf: Vec<u8>,
    skip_partial: bool,
    backlog: u64,
    started: bool,
}

impl Tail {
    /// `backlog`: how many bytes from the end to read on the first poll.
    pub fn new(path: PathBuf, backlog: u64) -> Self {
        Tail { path, pos: 0, ino: 0, buf: Vec::new(), skip_partial: false, backlog, started: false }
    }

    pub fn poll(&mut self) -> Vec<String> {
        let Ok(meta) = std::fs::metadata(&self.path) else { return Vec::new() };
        let len = meta.len();

        if !self.started {
            self.started = true;
            self.ino = meta.ino();
            self.pos = len.saturating_sub(self.backlog);
            self.skip_partial = self.pos > 0;
        } else if meta.ino() != self.ino || len < self.pos {
            // rotated or truncated: start over on the new file
            self.ino = meta.ino();
            self.pos = 0;
            self.buf.clear();
            self.skip_partial = false;
        }
        if len <= self.pos {
            return Vec::new();
        }
        if len - self.pos > MAX_READ {
            self.pos = len - MAX_READ;
            self.buf.clear();
            self.skip_partial = true;
        }

        let Ok(mut f) = File::open(&self.path) else { return Vec::new() };
        if f.seek(SeekFrom::Start(self.pos)).is_err() {
            return Vec::new();
        }
        let start = self.buf.len();
        let n = f.take(len - self.pos).read_to_end(&mut self.buf).unwrap_or(0);
        self.pos += n as u64;

        // Only hand out complete lines; keep the unterminated tail for next time.
        let Some(last_nl) = self.buf[start..].iter().rposition(|&b| b == b'\n').map(|i| i + start) else {
            return Vec::new();
        };
        let rest = self.buf.split_off(last_nl + 1);
        let complete = std::mem::replace(&mut self.buf, rest);

        let mut out = Vec::new();
        for line in complete[..complete.len() - 1].split(|&b| b == b'\n') {
            if self.skip_partial {
                self.skip_partial = false;
                continue;
            }
            let s = String::from_utf8_lossy(line);
            out.push(s.trim_end_matches('\r').to_string());
        }
        out
    }
}
