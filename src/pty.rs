// PTY (pseudo-terminal) management for the built-in terminal feature.
//
// Architecture:
//   PtyHandle::spawn()  →  forks bash via portable-pty, sets workdir
//   PtyHandle::write()  →  writes user input to the PTY master
//   PtyHandle::resize() →  updates terminal dimensions
//   PtyHandle::close()  →  kills the child process
//
// Reading from the PTY master is done via a background std::thread that
// sends output chunks through an mpsc channel back to the worker.

use portable_pty::{native_pty_system, CommandBuilder, Child, MasterPty, PtySize};
use std::io::{Read, Write};
use std::path::PathBuf;
use tokio::sync::mpsc;

pub struct PtyHandle {
    /// The PTY master (for writing input and reading output).
    master: Box<dyn MasterPty + Send>,
    /// Cached writer — `take_writer()` can only be called once.
    writer: Option<Box<dyn std::io::Write + Send>>,
    /// The spawned child process — kept alive to prevent SIGHUP.
    _child: Box<dyn Child + Send + 'static>,
    /// PID of the child process.
    pub child_pid: u32,
    /// Receiver for PTY output (read by the worker, forwarded to WebSocket).
    output_rx: mpsc::Receiver<Vec<u8>>,
    /// Signal to stop the read loop.
    _close_tx: tokio::sync::oneshot::Sender<()>,
}

impl PtyHandle {
    /// Spawn a new PTY running the user's shell (`$SHELL` or `bash`) in the
    /// given working directory. Runs the blocking fork+read loop on a background
    /// thread managed by tokio.
    pub fn spawn(workdir: PathBuf, rows: u16, cols: u16) -> Result<Self, String> {
        let pty_system = native_pty_system();

        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };

        let mut pair = pty_system
            .openpty(size)
            .map_err(|e| format!("Failed to open PTY: {}", e))?;

        // Determine shell from $SHELL, fallback to bash
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "bash".to_string());

        let mut cmd = CommandBuilder::new(&shell);
        // Force interactive mode so the shell shows a prompt and waits for input
        cmd.arg("-i");
        cmd.cwd(workdir);
        cmd.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn shell '{}': {}", shell, e))?;

        let child_pid = child
            .process_id()
            .unwrap_or(0);

        let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(256);
        let (close_tx, mut close_rx) = tokio::sync::oneshot::channel::<()>();

        // Spawn a blocking reader thread for PTY output
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to clone PTY reader: {}", e))?;

        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                // Check for close signal
                if close_rx.try_recv().is_ok() {
                    break;
                }
                match reader.read(&mut buf) {
                    Ok(0) => break, // EOF — child exited
                    Ok(n) => {
                        let chunk = buf[..n].to_vec();
                        // If channel is full/dropped, stop
                        if output_tx.blocking_send(chunk).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Take the writer ONCE here — MasterPty::take_writer() can only
        // be called a single time.
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to take PTY writer: {}", e))?;

        Ok(PtyHandle {
            master: pair.master,
            writer: Some(writer),
            _child: child,
            child_pid,
            output_rx,
            _close_tx: close_tx,
        })
    }

    /// Write user input to the PTY master.
    pub fn write(&mut self, data: &[u8]) -> Result<(), String> {
        match self.writer.as_mut() {
            Some(w) => w
                .write_all(data)
                .map_err(|e| format!("PTY write error: {}", e)),
            None => Err("PTY writer already consumed".to_string()),
        }
    }

    /// Resize the PTY.
    pub fn resize(&mut self, rows: u16, cols: u16) -> Result<(), String> {
        let size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        self.master
            .resize(size)
            .map_err(|e| format!("PTY resize error: {}", e))
    }

    /// Kill the child process.
    pub fn close(&mut self) -> Result<(), String> {
        #[cfg(unix)]
        unsafe {
            libc::kill(self.child_pid as i32, libc::SIGKILL);
        }
        #[cfg(not(unix))]
        {
            // On Windows the child is managed by conpty; dropping the master
            // usually terminates it.
            let _ = self.master.try_clone_reader();
        }
        Ok(())
    }

    /// Take the output receiver (moved into the worker's read loop).
    pub fn take_output_rx(&mut self) -> mpsc::Receiver<Vec<u8>> {
        std::mem::replace(&mut self.output_rx, mpsc::channel(1).1)
    }
}
