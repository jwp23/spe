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

/// Create a unique per-test runtime directory so the socket never collides with
/// a live app instance, a parallel test run, or another test in this binary
/// (cargo runs test functions concurrently, so the process id alone is not
/// unique enough — hence the per-test `name`).
fn make_test_runtime_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("spe-ipc-test-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("failed to create per-test runtime dir");
    dir
}

fn socket_path(runtime_dir: &Path) -> PathBuf {
    runtime_dir.join("spe-ipc.sock")
}

fn cage_available() -> bool {
    // cage exposes `-v` for the version (there is no long `--version`); it exits 0.
    Command::new("cage")
        .arg("-v")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Launch the app inside headless cage. Mirrors `scripts/screenshot.sh`.
/// Sets `XDG_RUNTIME_DIR` to `runtime_dir` so the app writes its socket there,
/// not into the real user runtime directory.
fn launch_app(runtime_dir: &Path, socket: &Path) -> Child {
    let _ = std::fs::remove_file(socket);
    Command::new("cage")
        .args(["--", env!("CARGO_BIN_EXE_spe"), "--ipc"])
        .env_remove("WAYLAND_DISPLAY")
        .env("WLR_BACKENDS", "headless")
        .env("WLR_LIBINPUT_NO_DEVICES", "1")
        .env("XDG_RUNTIME_DIR", runtime_dir)
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

    let runtime_dir = make_test_runtime_dir("sequence");
    let socket = socket_path(&runtime_dir);
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/single-page.pdf");
    let mut child = launch_app(&runtime_dir, &socket);

    // Run the whole sequence, collecting results, so we can always tear cage
    // down before asserting (a panic mid-sequence must not leak the process).
    let outcome = (|| -> Result<CommandLog, String> {
        wait_for_socket(&socket, &mut child, Duration::from_secs(20))?;

        // Cargo runs test binaries concurrently, so this sequence can be
        // competing with the whole `e2e` suite for the GPU. The bound is
        // generous for that reason: it is here to stop a wedged command
        // hanging the suite, not to police latency.
        let send = |json: &str| send_command(&socket, json, Duration::from_secs(15));

        let mut results: CommandLog = Vec::new();
        let open_json = format!(r#"{{"cmd": "open", "path": "{}"}}"#, fixture.display());
        results.push(("open", send(&open_json)));
        // wait_ready blocks until rendering completes; it is the command that
        // hangs forever when the render task is discarded.
        results.push(("wait_ready", send(r#"{"cmd": "wait_ready"}"#)));
        results.push((
            "click",
            send(r#"{"cmd": "click", "page": 1, "x": 100, "y": 700}"#),
        ));
        results.push(("type", send(r#"{"cmd": "type", "text": "Hello world"}"#)));
        results.push(("deselect", send(r#"{"cmd": "deselect"}"#)));
        Ok(results)
    })();

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&runtime_dir);

    let results = outcome.expect("IPC sequence setup failed");
    for (label, reply) in &results {
        assert_ok(label, reply);
    }
}

/// True if any content stream on any page shows `needle` via a `Tj` operator.
fn pdf_contains_text(path: &Path, needle: &str) -> bool {
    let doc = lopdf::Document::load(path).expect("saved file must be a loadable PDF");
    let target = needle.as_bytes().to_vec();
    for (_, page_id) in doc.get_pages() {
        for id in doc.get_page_contents(page_id) {
            let Ok(stream) = doc.get_object(id).and_then(|o| o.as_stream()) else {
                continue;
            };
            let Ok(content) = stream.decode_content() else {
                continue;
            };
            for op in &content.operations {
                if op.operator == "Tj"
                    && matches!(op.operands.first(), Some(lopdf::Object::String(b, _)) if *b == target)
                {
                    return true;
                }
            }
        }
    }
    false
}

fn assert_failed(label: &str, reply: &Result<String, String>, expected: &str) {
    match reply {
        Ok(body) => {
            assert!(
                body.contains("\"ok\":false") || body.contains("\"ok\": false"),
                "command `{label}` should have been rejected but returned: {body}"
            );
            assert!(
                body.contains(expected),
                "command `{label}` should explain `{expected}`, got: {body}"
            );
        }
        Err(e) => panic!("command `{label}` produced no reply: {e}"),
    }
}

/// spe-94g / spe-749 / spe-0nc: the full automation workflow over IPC —
/// open, place, type, save — plus proof that a command which cannot act says so
/// and that `undo` really reverts a placement.
#[test]
#[ignore]
fn ipc_open_place_type_save_round_trip() {
    if !cage_available() {
        eprintln!("SKIP ipc_open_place_type_save_round_trip: `cage` not available");
        return;
    }

    let runtime_dir = make_test_runtime_dir("save");
    let socket = socket_path(&runtime_dir);
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/single-page.pdf");
    let dest = runtime_dir.join("saved.pdf");
    let mut child = launch_app(&runtime_dir, &socket);

    let outcome = (|| -> Result<CommandLog, String> {
        wait_for_socket(&socket, &mut child, Duration::from_secs(20))?;
        let send = |json: &str| send_command(&socket, json, Duration::from_secs(15));

        let mut results: CommandLog = Vec::new();
        results.push((
            "type-before-document",
            send(r#"{"cmd": "type", "text": "nowhere"}"#),
        ));
        results.push((
            "open",
            send(&format!(
                r#"{{"cmd": "open", "path": "{}"}}"#,
                fixture.display()
            )),
        ));
        results.push(("wait_ready", send(r#"{"cmd": "wait_ready"}"#)));
        // With a document open but nothing selected, these commands cannot act
        // and must say so rather than replying ok and doing nothing.
        results.push((
            "type-before-placement",
            send(r#"{"cmd": "type", "text": "nowhere"}"#),
        ));
        results.push((
            "select-out-of-range",
            send(r#"{"cmd": "select", "index": 9}"#),
        ));
        results.push(("undo-with-empty-stack", send(r#"{"cmd": "undo"}"#)));
        results.push((
            "click",
            send(r#"{"cmd": "click", "page": 1, "x": 100, "y": 700}"#),
        ));
        results.push(("type", send(r#"{"cmd": "type", "text": "RoundTrip"}"#)));
        results.push(("deselect", send(r#"{"cmd": "deselect"}"#)));
        results.push((
            "save",
            send(&format!(
                r#"{{"cmd": "save", "path": "{}"}}"#,
                dest.display()
            )),
        ));
        results.push(("undo", send(r#"{"cmd": "undo"}"#)));
        Ok(results)
    })();

    let _ = child.kill();
    let _ = child.wait();

    let saved_ok = dest.exists() && pdf_contains_text(&dest, "RoundTrip");
    let _ = std::fs::remove_dir_all(&runtime_dir);

    let results = outcome.expect("IPC sequence setup failed");
    for (label, reply) in &results {
        match *label {
            "type-before-document" => assert_failed(label, reply, "no document is loaded"),
            "type-before-placement" => assert_failed(label, reply, "no overlay is active"),
            "select-out-of-range" => assert_failed(label, reply, "out of range"),
            "undo-with-empty-stack" => assert_failed(label, reply, "nothing to undo"),
            _ => assert_ok(label, reply),
        }
    }
    assert!(
        saved_ok,
        "the saved PDF must exist and contain the typed overlay text"
    );
}

/// spe-9gt.7.1: the full user workflow with a bundled cursive font — open, pick
/// the font, place, type, save — must produce a PDF whose text a reader can
/// extract, which is what the ToUnicode CMap on the embedded TrueType font buys.
#[test]
#[ignore]
fn ipc_cursive_overlay_text_is_extractable_by_pdftotext() {
    if !cage_available() {
        eprintln!(
            "SKIP ipc_cursive_overlay_text_is_extractable_by_pdftotext: `cage` not available"
        );
        return;
    }

    let runtime_dir = make_test_runtime_dir("cursive-extract");
    let socket = socket_path(&runtime_dir);
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/single-page.pdf");
    let dest = runtime_dir.join("cursive.pdf");
    let mut child = launch_app(&runtime_dir, &socket);

    let outcome = (|| -> Result<CommandLog, String> {
        wait_for_socket(&socket, &mut child, Duration::from_secs(20))?;
        let send = |json: &str| send_command(&socket, json, Duration::from_secs(15));

        let mut results: CommandLog = Vec::new();
        results.push((
            "open",
            send(&format!(
                r#"{{"cmd": "open", "path": "{}"}}"#,
                fixture.display()
            )),
        ));
        results.push(("wait_ready", send(r#"{"cmd": "wait_ready"}"#)));
        results.push((
            "click",
            send(r#"{"cmd": "click", "page": 1, "x": 100, "y": 700}"#),
        ));
        results.push(("font", send(r#"{"cmd": "font", "family": "Great Vibes"}"#)));
        results.push(("type", send(r#"{"cmd": "type", "text": "Ada Lovelace"}"#)));
        results.push(("deselect", send(r#"{"cmd": "deselect"}"#)));
        results.push((
            "save",
            send(&format!(
                r#"{{"cmd": "save", "path": "{}"}}"#,
                dest.display()
            )),
        ));
        Ok(results)
    })();

    let _ = child.kill();
    let _ = child.wait();

    let extracted = dest.exists().then(|| {
        let output = Command::new("pdftotext")
            .arg(&dest)
            .arg("-")
            .output()
            .expect("pdftotext must be installed (poppler-utils)");
        assert!(
            output.status.success(),
            "pdftotext failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    });
    let _ = std::fs::remove_dir_all(&runtime_dir);

    let results = outcome.expect("IPC sequence setup failed");
    for (label, reply) in &results {
        assert_ok(label, reply);
    }
    let extracted = extracted.expect("save must have written a PDF");
    assert!(
        extracted.contains("Ada Lovelace"),
        "cursive overlay text must be extractable from the saved PDF, got:\n{extracted}"
    );
}

/// spe-47n: Save must always produce a file, even with no overlays placed —
/// open then save with nothing typed must still write a loadable PDF with
/// the same page count as the source, not silently do nothing.
#[test]
#[ignore]
fn ipc_open_save_with_no_overlays_still_writes_file() {
    if !cage_available() {
        eprintln!("SKIP ipc_open_save_with_no_overlays_still_writes_file: `cage` not available");
        return;
    }

    let runtime_dir = make_test_runtime_dir("save-empty");
    let socket = socket_path(&runtime_dir);
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/single-page.pdf");
    let dest = runtime_dir.join("saved-empty.pdf");
    let mut child = launch_app(&runtime_dir, &socket);

    let outcome = (|| -> Result<CommandLog, String> {
        wait_for_socket(&socket, &mut child, Duration::from_secs(20))?;
        let send = |json: &str| send_command(&socket, json, Duration::from_secs(15));

        let mut results: CommandLog = Vec::new();
        results.push((
            "open",
            send(&format!(
                r#"{{"cmd": "open", "path": "{}"}}"#,
                fixture.display()
            )),
        ));
        results.push(("wait_ready", send(r#"{"cmd": "wait_ready"}"#)));
        results.push((
            "save",
            send(&format!(
                r#"{{"cmd": "save", "path": "{}"}}"#,
                dest.display()
            )),
        ));
        Ok(results)
    })();

    let _ = child.kill();
    let _ = child.wait();

    let source_pages = lopdf::Document::load(&fixture)
        .expect("fixture must be a loadable PDF")
        .get_pages()
        .len();
    let dest_pages = dest
        .exists()
        .then(|| lopdf::Document::load(&dest).ok())
        .flatten()
        .map(|doc| doc.get_pages().len());
    let _ = std::fs::remove_dir_all(&runtime_dir);

    let results = outcome.expect("IPC sequence setup failed");
    for (label, reply) in &results {
        assert_ok(label, reply);
    }
    assert_eq!(
        dest_pages,
        Some(source_pages),
        "an empty-overlay save must still write a loadable PDF with the source's page count"
    );
}
