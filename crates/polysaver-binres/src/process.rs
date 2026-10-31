// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 PolySaver contributors

//! Process spawning helpers shared by every adapter that runs a sidecar.

use std::ffi::OsStr;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};
use tokio::process::Command;

/// `CREATE_NO_WINDOW` process creation flag (winbase.h).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Builds a [`Command`] that never pops up a console window on Windows.
///
/// The release build uses the `windows` subsystem, so it has no console to
/// share: without this flag Windows opens a new, visible one for every console
/// program it runs (yt-dlp, ffmpeg, ffprobe, node). The hidden console is
/// inherited by grandchildren too (ffmpeg spawned by yt-dlp for muxing), and
/// piped stdout/stderr keep working. Identical to `Command::new` elsewhere.
pub fn background_command(program: impl AsRef<OsStr>) -> Command {
    #[cfg_attr(not(windows), allow(unused_mut))]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Reads the next line from a child's output pipe, without its line ending.
///
/// Invalid UTF-8 is decoded lossily instead of ending the stream: tokio's
/// `Lines` stops at the first undecodable byte, and once a pipe is no longer
/// drained the child fails on its next write (yt-dlp then dies with EINVAL on
/// Windows, where it may write in the ANSI code page). Returns `None` at EOF or
/// on an I/O error.
pub async fn read_line_lossy<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    buf: &mut Vec<u8>,
) -> Option<String> {
    buf.clear();
    match reader.read_until(b'\n', buf).await {
        Ok(0) | Err(_) => None,
        Ok(_) => {
            while matches!(buf.last(), Some(b'\n' | b'\r')) {
                buf.pop();
            }
            Some(String::from_utf8_lossy(buf).into_owned())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_read_line_lossy_survives_invalid_utf8() {
        // "perturbé" in cp1252 (0xE9), then a valid UTF-8 line and a final line
        // without a trailing newline.
        let input: &[u8] = b"title:perturb\xe9\r\n[POLYSAVER_OUTPUT] /dl/vid\xc3\xa9o.mp4\nlast";
        let mut reader = tokio::io::BufReader::new(input);
        let mut buf = Vec::new();

        assert_eq!(
            read_line_lossy(&mut reader, &mut buf).await.as_deref(),
            Some("title:perturb\u{fffd}")
        );
        assert_eq!(
            read_line_lossy(&mut reader, &mut buf).await.as_deref(),
            Some("[POLYSAVER_OUTPUT] /dl/vidéo.mp4")
        );
        assert_eq!(
            read_line_lossy(&mut reader, &mut buf).await.as_deref(),
            Some("last")
        );
        assert_eq!(read_line_lossy(&mut reader, &mut buf).await, None);
    }
}
