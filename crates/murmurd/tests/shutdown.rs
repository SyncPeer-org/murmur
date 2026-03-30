//! Integration test: murmurd must exit promptly on SIGINT (Ctrl+C).

use std::process::Command;
use std::time::{Duration, Instant};

/// Build the murmurd binary (debug) and return the path.
fn murmurd_bin() -> std::path::PathBuf {
    let status = Command::new("cargo")
        .args(["build", "--bin", "murmurd"])
        .status()
        .expect("cargo build");
    assert!(status.success(), "murmurd build failed");

    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop(); // crates/
    path.pop(); // repo root
    path.push("target/debug/murmurd");
    assert!(path.exists(), "murmurd binary not found at {path:?}");
    path
}

#[test]
fn test_sigint_exits_promptly() {
    let bin = murmurd_bin();
    let data_dir = tempfile::tempdir().expect("tempdir");
    let sock_path = data_dir.path().join("murmurd.sock");

    let mut child = Command::new(&bin)
        .args(["--data-dir", data_dir.path().to_str().unwrap()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn murmurd");

    let pid = child.id();

    // Wait for the socket file to appear (daemon is listening).
    let startup_deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if sock_path.exists() {
            break;
        }
        if Instant::now() > startup_deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("murmurd did not create socket within 30 seconds");
        }
        // Check the process hasn't crashed.
        if let Some(status) = child.try_wait().expect("try_wait") {
            panic!("murmurd exited during startup with {status}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Send SIGINT (same as Ctrl+C).
    unsafe {
        libc::kill(pid as i32, libc::SIGINT);
    }

    // The daemon should exit within 5 seconds.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait().expect("try_wait") {
            Some(status) => {
                eprintln!("murmurd exited with {status}");
                return;
            }
            None => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!(
                        "murmurd did not exit within 5 seconds after SIGINT — \
                         shutdown is hanging (likely spawned tasks not cancelled)"
                    );
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}
