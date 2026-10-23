//! Bounded framing for nonblocking Unix sockets and Windows named pipes.
use aipass_agent_protocol::MAX_FRAME_BYTES;
use anyhow::{bail, Result};
use interprocess::local_socket::traits::Stream as _;
use interprocess::local_socket::Stream;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    io::{ErrorKind, Read, Write},
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroize;

pub fn read_frame_with_deadline<T: DeserializeOwned>(
    conn: &mut Stream,
    timeout: Duration,
) -> Result<T> {
    conn.set_nonblocking(true)?;
    let mut conn = FrameIo::new(conn);
    let deadline = Instant::now() + timeout;
    let mut len = [0_u8; 4];
    read_exact_with_deadline(&mut conn, &mut len, deadline)?;
    let len = u32::from_le_bytes(len) as usize;
    if len > MAX_FRAME_BYTES {
        bail!("frame too large");
    }
    let mut body = vec![0_u8; len];
    if let Err(err) = read_exact_with_deadline(&mut conn, &mut body, deadline) {
        body.zeroize();
        return Err(err);
    }
    let parsed = serde_json::from_slice(&body);
    body.zeroize();
    Ok(parsed?)
}

pub fn write_frame_with_deadline<T: Serialize>(
    conn: &mut Stream,
    value: &T,
    timeout: Duration,
) -> Result<()> {
    conn.set_nonblocking(true)?;
    let mut conn = FrameIo::new(conn);
    let mut body = serde_json::to_vec(value)?;
    if body.len() > MAX_FRAME_BYTES {
        body.zeroize();
        bail!("frame too large");
    }
    let deadline = Instant::now() + timeout;
    let len = (body.len() as u32).to_le_bytes();
    let result = write_all_with_deadline(&mut conn, &len, deadline)
        .and_then(|_| write_all_with_deadline(&mut conn, &body, deadline));
    body.zeroize();
    result
}

struct FrameIo<'a> {
    stream: &'a mut Stream,
    #[cfg(windows)]
    write_limit: usize,
}

impl<'a> FrameIo<'a> {
    fn new(stream: &'a mut Stream) -> Self {
        Self {
            stream,
            #[cfg(windows)]
            write_limit: usize::MAX,
        }
    }
}

impl Read for FrameIo<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        #[cfg(windows)]
        {
            use std::os::windows::io::{AsHandle, AsRawHandle};
            use windows_sys::Win32::System::Pipes::PeekNamedPipe;
            let Stream::NamedPipe(pipe) = &*self.stream;
            let mut available = 0;
            // PIPE_NOWAIT reads on an empty pipe return ERROR_NO_DATA, which
            // std/interprocess treat as EOF, rather than WouldBlock. Check first
            // and read only bytes already available. Each frame has one reader.
            if unsafe {
                PeekNamedPipe(
                    pipe.as_handle().as_raw_handle(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut available,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error());
            }
            if available == 0 {
                return Err(ErrorKind::WouldBlock.into());
            }
            let length = buffer.len().min(available as usize);
            self.stream.read(&mut buffer[..length])
        }
        #[cfg(not(windows))]
        self.stream.read(buffer)
    }
}

impl Write for FrameIo<'_> {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        #[cfg(windows)]
        let buffer = &buffer[..buffer.len().min(self.write_limit)];
        let result = self.stream.write(buffer);
        // Some PIPE_NOWAIT implementations cannot partially accept a write
        // larger than the pipe buffer, even after the reader drains it. Retrying
        // that same buffer would never progress. Fragment only after observing
        // zero progress; 512 bytes is interprocess's default pipe buffer size.
        #[cfg(windows)]
        if matches!(result, Ok(0)) && buffer.len() > 512 {
            self.write_limit = 512;
            return match self.stream.write(&buffer[..512]) {
                Ok(0) => Err(ErrorKind::WouldBlock.into()),
                result => result,
            };
        }
        // Byte-mode PIPE_NOWAIT can successfully write zero bytes when full.
        // That is backpressure, not WriteZero or a closed peer.
        #[cfg(windows)]
        if matches!(result, Ok(0)) && !buffer.is_empty() {
            return Err(ErrorKind::WouldBlock.into());
        }
        result
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.stream.flush()
    }
}

fn read_exact_with_deadline(conn: &mut impl Read, buf: &mut [u8], deadline: Instant) -> Result<()> {
    let mut offset = 0;
    let mut last_progress = Instant::now();
    while offset < buf.len() {
        check_deadline(deadline)?;
        match conn.read(&mut buf[offset..]) {
            Ok(0) => {
                return Err(std::io::Error::from(ErrorKind::UnexpectedEof).into());
            }
            Ok(count) => {
                offset += count;
                last_progress = Instant::now();
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                wait_for_io(deadline, last_progress)?
            }
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

fn write_all_with_deadline(conn: &mut impl Write, buf: &[u8], deadline: Instant) -> Result<()> {
    let mut offset = 0;
    let mut last_progress = Instant::now();
    while offset < buf.len() {
        check_deadline(deadline)?;
        match conn.write(&buf[offset..]) {
            Ok(0) => {
                return Err(std::io::Error::from(ErrorKind::WriteZero).into());
            }
            Ok(count) => {
                offset += count;
                last_progress = Instant::now();
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) if err.kind() == ErrorKind::WouldBlock => {
                wait_for_io(deadline, last_progress)?
            }
            Err(err) => return Err(err.into()),
        }
    }
    Ok(())
}

fn wait_for_io(deadline: Instant, last_progress: Instant) -> Result<()> {
    let now = Instant::now();
    check_deadline(deadline)?;
    // Give an active reader/writer time to drain the small pipe buffer before
    // sleeping. Sleeping 5 ms for every 512-byte fragment imposes a >5 s floor
    // on a 512 KiB response, even when neither peer is slow. After a short idle
    // interval, sleep as before so a silent peer does not cause a busy loop.
    if now.duration_since(last_progress) < Duration::from_millis(1) {
        thread::yield_now();
    } else {
        thread::sleep((deadline - now).min(Duration::from_millis(5)));
    }
    Ok(())
}

fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        return Err(std::io::Error::new(ErrorKind::TimedOut, "agent IPC timed out").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_buffer_backpressure_does_not_consume_the_frame_deadline() {
        use std::{
            collections::VecDeque,
            sync::{Arc, Mutex},
        };
        #[derive(Clone)]
        struct SmallPipe(Arc<Mutex<VecDeque<u8>>>);
        impl Read for SmallPipe {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let mut queue = self.0.lock().unwrap();
                let count = buffer.len().min(queue.len());
                if count == 0 {
                    return Err(ErrorKind::WouldBlock.into());
                }
                for byte in &mut buffer[..count] {
                    *byte = queue.pop_front().unwrap();
                }
                Ok(count)
            }
        }
        impl Write for SmallPipe {
            fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
                let mut queue = self.0.lock().unwrap();
                let count = buffer.len().min(512 - queue.len());
                if count == 0 {
                    return Err(ErrorKind::WouldBlock.into());
                }
                queue.extend(&buffer[..count]);
                Ok(count)
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut writer = SmallPipe(Arc::new(Mutex::new(VecDeque::new())));
        let mut reader = writer.clone();
        let deadline = Instant::now() + Duration::from_secs(5);
        let payload = vec![42; 1024 * 1024];
        let sent = payload.clone();
        let producer = thread::spawn(move || write_all_with_deadline(&mut writer, &sent, deadline));
        let mut received = vec![0; payload.len()];
        let result = read_exact_with_deadline(&mut reader, &mut received, deadline);
        producer.join().unwrap().unwrap();
        result.unwrap();
        assert_eq!(received, payload);
    }

    #[cfg(windows)]
    #[test]
    fn named_pipe_frames_larger_than_the_pipe_buffer_round_trip() {
        use interprocess::local_socket::traits::Listener as _;
        let dir = tempfile::tempdir().unwrap();
        let listener = crate::ipc::listen(dir.path()).unwrap();
        let server = thread::spawn(move || {
            let mut stream = listener.accept().unwrap();
            let payload: String =
                read_frame_with_deadline(&mut stream, Duration::from_secs(5)).unwrap();
            assert_eq!(payload, "中文".repeat(8192));
            write_frame_with_deadline(&mut stream, &payload, Duration::from_secs(5)).unwrap();
            thread::sleep(Duration::from_millis(100));
        });
        let mut stream = crate::ipc::connect(dir.path()).unwrap();
        let payload = "中文".repeat(8192);
        write_frame_with_deadline(&mut stream, &payload, Duration::from_secs(5)).unwrap();
        let reply: String = read_frame_with_deadline(&mut stream, Duration::from_secs(5)).unwrap();
        assert_eq!(reply, payload);
        server.join().unwrap();
    }
    struct Blocked;
    impl Read for Blocked {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(ErrorKind::WouldBlock.into())
        }
    }
    impl Write for Blocked {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(ErrorKind::WouldBlock.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    #[test]
    fn nonblocking_io_has_a_real_timeout_error() {
        for writing in [false, true] {
            let deadline = Instant::now() + Duration::from_millis(20);
            let result = if writing {
                write_all_with_deadline(&mut Blocked, b"x", deadline)
            } else {
                read_exact_with_deadline(&mut Blocked, &mut [0], deadline)
            };
            assert_eq!(
                result
                    .unwrap_err()
                    .downcast_ref::<std::io::Error>()
                    .unwrap()
                    .kind(),
                ErrorKind::TimedOut
            );
        }
    }
}
