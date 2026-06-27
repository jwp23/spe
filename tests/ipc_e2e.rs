// End-to-end IPC test: drives the real app over its Unix-socket IPC protocol
// inside a headless `cage` compositor, exactly as the screenshot harness does.
//
// This is the only level that exercises async `Task` execution through the real
// iced event loop, which is where the spe-dr0 bug lives: a command whose update
// returns a follow-up `Task` (e.g. `open`, which renders pages) had that task
// discarded, so rendering never finished, `wait_ready` never got a response, and
// — because the IPC accept loop handles connections serially and blocks on the
// response — every later command was wedged too.
//
// Requires `cage` and a GPU/Wayland session, so it is `#[ignore]` and skips
// cleanly when `cage` is absent (e.g. CI without the screenshot deps).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// One command's label paired with its reply (or the error explaining the failure).
type CommandLog = Vec<(&'static str, Result<String, String>)>;

fn socket_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(dir).join("spe-ipc.sock")
}

fn cage_available() -> bool {
    Command::new("cage")
        .arg("--version")
        .output()
        .map(|o| o.status.success() || o.status.code().is_some())
        .unwrap_or(false)
}

/// Launch the app inside headless cage. Mirrors `scripts/screenshot.sh`.
fn launch_app(socket: &Path) -> Child {
    let _ = std::fs::remove_file(socket);
    Command::new("cage")
        .args(["--", env!("CARGO_BIN_EXE_spe"), "--ipc"])
        .env_remove("WAYLAND_DISPLAY")
        .env("WLR_BACKENDS", "headless")
        .env("WLR_LIBINPUT_NO_DEVICES", "1")
        .spawn()
        .expect("failed to spawn cage")
}

/// Poll until the IPC socket exists or we give up.
fn wait_for_socket(socket: &Path, child: &mut Child, timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if socket.exists() {
            return Ok(());
        }
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("app exited before binding socket: {status}"));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("timed out waiting for IPC socket".to_string())
}

/// Send one JSON command and read the single-line JSON reply. Returns the raw
/// reply string, or an error (including timeout) describing what went wrong.
fn send_command(socket: &Path, json: &str, timeout: Duration) -> Result<String, String> {
    let stream = UnixStream::connect(socket).map_err(|e| format!("connect failed: {e}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| format!("set_read_timeout failed: {e}"))?;

    let mut line = String::from(json);
    line.push('\n');
    (&stream)
        .write_all(line.as_bytes())
        .map_err(|e| format!("write failed: {e}"))?;

    let mut reply = String::new();
    BufReader::new(&stream)
        .read_line(&mut reply)
        .map_err(|e| format!("no reply (timeout/error): {e}"))?;
    if reply.trim().is_empty() {
        return Err("connection closed without a reply".to_string());
    }
    Ok(reply.trim().to_string())
}

fn assert_ok(label: &str, reply: &Result<String, String>) {
    match reply {
        Ok(body) => assert!(
            body.contains("\"ok\":true") || body.contains("\"ok\": true"),
            "command `{label}` did not return ok=true: {body}"
        ),
        Err(e) => panic!("command `{label}` failed: {e}"),
    }
}

#[test]
#[ignore]
fn ipc_command_sequence_all_receive_responses() {
    if !cage_available() {
        eprintln!("SKIP ipc_command_sequence_all_receive_responses: `cage` not available");
        return;
    }

    let socket = socket_path();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/single-page.pdf");
    let mut child = launch_app(&socket);

    // Run the whole sequence, collecting results, so we can always tear cage
    // down before asserting (a panic mid-sequence must not leak the process).
    let outcome = (|| -> Result<CommandLog, String> {
        wait_for_socket(&socket, &mut child, Duration::from_secs(20))?;

        let mut results: CommandLog = Vec::new();
        let open_json = format!(r#"{{"cmd": "open", "path": "{}"}}"#, fixture.display());
        results.push((
            "open",
            send_command(&socket, &open_json, Duration::from_secs(5)),
        ));
        // wait_ready blocks until rendering completes; it is the command that
        // hangs forever when the render task is discarded.
        results.push((
            "wait_ready",
            send_command(&socket, r#"{"cmd": "wait_ready"}"#, Duration::from_secs(15)),
        ));
        results.push((
            "click",
            send_command(
                &socket,
                r#"{"cmd": "click", "page": 1, "x": 100, "y": 700}"#,
                Duration::from_secs(5),
            ),
        ));
        results.push((
            "type",
            send_command(
                &socket,
                r#"{"cmd": "type", "text": "Hello world"}"#,
                Duration::from_secs(5),
            ),
        ));
        results.push((
            "deselect",
            send_command(&socket, r#"{"cmd": "deselect"}"#, Duration::from_secs(5)),
        ));
        Ok(results)
    })();

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&socket);

    let results = outcome.expect("IPC sequence setup failed");
    for (label, reply) in &results {
        assert_ok(label, reply);
    }
}
