//! Private, bounded credential reader for the existing Elixir stdio server.
//! No network, child processes, environment identity or credential diagnostics.
mod owner_file;

use std::io::{BufRead, Read, Write};

fn main() {
    // SAFETY: alarm installs a process-local deadline; this binary has no threads
    // or children and retains SIGALRM's default terminating disposition.
    unsafe {
        libc::signal(libc::SIGALRM, libc::SIG_DFL);
        let mut signals = std::mem::zeroed();
        libc::sigemptyset(&mut signals);
        libc::sigaddset(&mut signals, libc::SIGALRM);
        libc::sigprocmask(libc::SIG_UNBLOCK, &signals, std::ptr::null_mut());
        libc::alarm(3);
    }
    let result = (|| {
        let mut args = std::env::args_os().skip(1);
        let first = args.next().ok_or(())?;
        let install = first == "--install";
        let path = if install {
            args.next().ok_or(())?
        } else {
            first
        };
        if args.next().is_some() {
            return Err(());
        }
        if install {
            // SAFETY: stdin is the fixed POSIX descriptor. Refuse interactive
            // input so a credential cannot be echoed into terminal scrollback.
            if unsafe { libc::isatty(libc::STDIN_FILENO) } != 0 {
                return Err(());
            }
            let mut bytes = Vec::new();
            std::io::stdin()
                .lock()
                .take(owner_file::MAX_BYTES + 1)
                .read_until(b'\n', &mut bytes)
                .map_err(|_| ())?;
            let config: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| ())?;
            if config["version"].as_u64() != Some(1)
                || !config["url"].is_string()
                || !config["token"].is_string()
                || !config["binding"].is_object()
            {
                return Err(());
            }
            return owner_file::install(std::path::Path::new(&path), &bytes);
        }
        let bytes = owner_file::read(std::path::Path::new(&path))?;
        std::io::stdout().lock().write_all(&bytes).map_err(|_| ())
    })();
    if result.is_err() {
        std::process::exit(1);
    }
}
