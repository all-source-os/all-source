use std::{
    fs::OpenOptions,
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

pub struct OwnedChild(Child);

impl OwnedChild {
    pub fn spawn(command: &mut Command) -> Result<Self, String> {
        command
            .spawn()
            .map(Self)
            .map_err(|_| "cannot start monitor process".into())
    }

    pub fn exited(&mut self) -> Result<bool, String> {
        self.0
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|_| "cannot read child status".into())
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        // An unreaped child retains its PID, so this cannot signal a reused PID.
        unsafe {
            libc::kill(self.0.id() as libc::pid_t, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if matches!(self.0.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct CaptureFile(std::path::PathBuf);

impl Drop for CaptureFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

pub fn capture(command: &mut Command, input: Option<&[u8]>) -> Result<Vec<u8>, String> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path = CaptureFile(std::env::temp_dir().join(format!(
        "monitor-capture-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path.0)
        .map_err(|_| "cannot create private command capture")?;
    command
        .stdout(Stdio::from(file))
        .stderr(Stdio::null())
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    let mut child = OwnedChild::spawn(command)?;
    if let Some(input) = input {
        let mut stdin = child.0.stdin.take().ok_or("child has no input")?;
        stdin
            .write_all(input)
            .map_err(|_| "cannot transfer private command input")?;
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = child.0.try_wait().map_err(|_| "cannot wait for command")? {
            if !status.success() {
                return Err(format!(
                    "command failed with {status}; output withheld to protect credentials"
                ));
            }
            break;
        }
        if Instant::now() >= deadline {
            return Err("command exceeded 60-second deadline".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut output = Vec::new();
    std::fs::File::open(&path.0)
        .map_err(|_| "cannot read command capture")?
        .take(65537)
        .read_to_end(&mut output)
        .map_err(|_| "cannot read command capture")?;
    if output.len() > 65536 {
        return Err("command output exceeded capture limit".into());
    }
    Ok(output)
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
