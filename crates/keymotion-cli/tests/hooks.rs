//! Hook mode and connecting, with the real `keymotion-cli` program: a fake
//! Keymotion on a test pipe (`KEYMOTION_CLI_TEST_PIPE`) and a test home
//! (`KEYMOTION_CLI_TEST_HOME`), so nothing reaches the real app or the
//! user's Claude Code and Codex settings.
//!
//! The hook lines `connect` writes are run the way the agents run them:
//! Claude Code's as a program and its arguments, Codex's through
//! PowerShell (its shell on Windows), from a folder with spaces in its name.

#![cfg(all(windows, any(debug_assertions, feature = "test-pipe")))]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command as Process, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use kb_agent::transport::serve;
use kb_agent::{AgentError, Channel, Command, ErrorCode, ServerInfo};
use kb_winsys::pipe::PipeServer;
use serde_json::{json, Value};

const EXE: &str = env!("CARGO_BIN_EXE_keymotion-cli");

fn unique(prefix: &str) -> String {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    format!("{prefix}{}-{}-{nanos}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed))
}

fn unique_pipe() -> String {
    unique(r"\\.\pipe\keymotion-cli-test-hooks-")
}

/// A fresh folder with a space in its name.
fn temp_home() -> PathBuf {
    let dir = std::env::temp_dir().join(unique("keymotion hooks test "));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

type Seen = Arc<Mutex<Vec<(String, Value)>>>;

/// A fake Keymotion that records every `hook` request: (source, args).
fn fake_app(pipe: &str, access: bool) -> (PipeServer, Seen) {
    let seen: Seen = Arc::default();
    let me = seen.clone();
    let info = ServerInfo { version: env!("CARGO_PKG_VERSION").into(), channel: Channel::Dev };
    let server = serve(pipe, info, move |call, command| match command {
        Command::Hook(args) => {
            args.validate()?;
            me.lock().unwrap().push((call.source.as_str().to_string(), serde_json::to_value(&args).unwrap()));
            if access {
                Ok(json!({"lit": true, "cleared": 0}))
            } else {
                Err(AgentError::new(ErrorCode::AgentAccessOff, "Agent access is off in Keymotion."))
            }
        }
        _ => Ok(json!({})),
    })
    .expect("the fake app serves its pipe");
    (server, seen)
}

#[derive(Debug)]
struct Run {
    code: i32,
    stdout: Vec<u8>,
    stderr: String,
    took: Duration,
}

/// Run `program` with `args`, `input` on stdin, the test pipe and home set
/// and the agents' own folder variables taken away.
fn run_program(program: &Path, args: &[&str], input: &[u8], pipe: &str, home: Option<&Path>) -> Run {
    let started = Instant::now();
    let mut cmd = Process::new(program);
    cmd.args(args)
        .env("KEYMOTION_CLI_TEST_PIPE", pipe)
        .env_remove("CLAUDE_CONFIG_DIR")
        .env_remove("CODEX_HOME")
        .env_remove("KEYMOTION_OUTPUT")
        .env_remove("KEYMOTION_HOOK_LOG")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match home {
        Some(home) => cmd.env("KEYMOTION_CLI_TEST_HOME", home),
        // Never the user's: a home that does not exist.
        None => cmd.env("KEYMOTION_CLI_TEST_HOME", std::env::temp_dir().join("keymotion-no-home")),
    };
    let mut child = cmd.spawn().expect("runs");
    {
        let mut stdin = child.stdin.take().unwrap();
        let _ = stdin.write_all(input);
    }
    let out = child.wait_with_output().unwrap();
    Run {
        code: out.status.code().expect("an exit code"),
        stdout: out.stdout,
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        took: started.elapsed(),
    }
}

fn hook(agent: &str, input: &[u8], pipe: &str) -> Run {
    let r = run_program(Path::new(EXE), &["hook", agent], input, pipe, None);
    assert_eq!(r.code, 0, "hook mode exits with 0: {r:?}");
    assert!(r.stdout.is_empty(), "hook mode prints nothing: {r:?}");
    assert!(r.stderr.is_empty(), "not even on stderr, unless KEYMOTION_HOOK_LOG asks: {r:?}");
    r
}

fn wait_for(seen: &Seen, n: usize, within: Duration) -> Vec<(String, Value)> {
    let started = Instant::now();
    while seen.lock().unwrap().len() < n && started.elapsed() < within {
        std::thread::sleep(Duration::from_millis(50));
    }
    seen.lock().unwrap().clone()
}

const STOP: &[u8] = br#"{"session_id":"s-1","transcript_path":"C:\\t.jsonl","cwd":"C:\\Users\\a\\dev\\my project","hook_event_name":"Stop","stop_hook_active":false,"last_assistant_message":"secret"}"#;

#[test]
fn every_kind_of_input_ends_with_0_and_no_output() {
    let pipe = unique_pipe();
    let (_server, seen) = fake_app(&pipe, true);
    let huge = {
        let mut v = br#"{"hook_event_name":"Stop","session_id":"s","pad":""#.to_vec();
        v.extend(std::iter::repeat_n(b'x', 17 * 1024 * 1024));
        v.extend(br#""}"#);
        v
    };
    for (agent, input) in [
        ("claude-code", &b"garbage"[..]),
        ("claude-code", b""),
        ("codex", b"{"),
        ("codex", b"\xff\xfe\x00"),
        ("claude-code", b"[1,2,3]"),
        ("claude-code", br#"{"hook_event_name":"Nope","session_id":"s"}"#),
        ("claude-code", br#"{"hook_event_name":"Notification","notification_type":"idle_prompt","session_id":"s"}"#),
        ("claude-code", &huge[..]),
        ("cursor", STOP),
        ("", STOP),
    ] {
        let r = hook(agent, input, &pipe);
        assert!(r.took < Duration::from_secs(5), "{agent}: {:?}", r.took);
    }
    assert!(seen.lock().unwrap().is_empty(), "none of these was an event: {:?}", seen.lock().unwrap());

    // Through clap, with options before it: still 0 and nothing printed.
    let r = run_program(Path::new(EXE), &["--json", "hook", "codex"], b"nonsense", &pipe, None);
    assert_eq!((r.code, r.stdout.len()), (0, 0), "{r:?}");
    let r = run_program(Path::new(EXE), &["hook"], b"", &pipe, None);
    assert_eq!((r.code, r.stdout.len()), (0, 0), "{r:?}");
    // From a closed stdin.
    let r = Process::new(EXE)
        .args(["hook", "claude-code"])
        .env("KEYMOTION_CLI_TEST_PIPE", &pipe)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!((r.status.code(), r.stdout.len()), (Some(0), 0));
}

#[test]
fn an_event_reaches_the_app_as_its_meaning_only() {
    let pipe = unique_pipe();
    let (_server, seen) = fake_app(&pipe, true);
    let r = hook("claude-code", STOP, &pipe);
    assert!(r.took < Duration::from_secs(2), "{:?}", r.took);
    let got = seen.lock().unwrap().clone();
    assert_eq!(got.len(), 1, "{got:?}");
    let (source, args) = &got[0];
    assert!(source.starts_with("claude-code:") && source.len() == "claude-code:".len() + 8, "{source}");
    assert_eq!(args, &json!({"event": "done", "agent": "claude-code", "label": "my project"}));
    let all = format!("{got:?}");
    for private in ["secret", "t.jsonl", "s-1", "Users"] {
        assert!(!all.contains(private), "{private} reached the app: {all}");
    }

    // A question and its answer name the same call.
    let ask = br#"{"session_id":"s-1","cwd":"C:\\p","hook_event_name":"PermissionRequest","tool_name":"Bash","tool_input":{"command":"cargo test","description":"Run the tests"}}"#;
    let done = br#"{"session_id":"s-1","cwd":"C:\\p","hook_event_name":"PostToolUse","tool_name":"Bash","tool_input":{"command":"cargo test"},"tool_use_id":"toolu_1","tool_response":{"stdout":"ok"}}"#;
    hook("claude-code", ask, &pipe);
    hook("claude-code", done, &pipe);
    let got = seen.lock().unwrap().clone();
    let asked = got[1].1["calls"].as_array().unwrap().clone();
    let answered = got[2].1["calls"].as_array().unwrap().clone();
    assert_eq!((got[1].1["event"].clone(), got[2].1["event"].clone()), (json!("attention"), json!("answered")));
    assert!(asked.iter().all(|k| answered.contains(k)), "{asked:?} {answered:?}");

    // Codex too; and agent access off is as silent as anything.
    let (_off, refused) = fake_app(&format!("{pipe}-off"), false);
    let codex_stop = br#"{"session_id":"thr_1","cwd":"/w","hook_event_name":"Stop","turn_id":"t","model":"m"}"#;
    hook("codex", codex_stop, &format!("{pipe}-off"));
    assert_eq!(refused.lock().unwrap()[0].1["event"], json!("done"));
}

#[test]
fn without_the_app_an_event_that_lights_is_delivered_once_it_starts() {
    let pipe = unique_pipe();
    // Nothing on the pipe: the hook does not wait for anything.
    let r = hook("claude-code", STOP, &pipe);
    assert!(r.took < Duration::from_secs(2), "{:?}", r.took);
    // A new prompt is not worth starting Keymotion for.
    let busy = br#"{"session_id":"s-2","cwd":"C:\\p","hook_event_name":"UserPromptSubmit","prompt":"x"}"#;
    hook("claude-code", busy, &pipe);
    // The app comes up (the test pipe starts nothing; the copy waits for it).
    std::thread::sleep(Duration::from_millis(700));
    let (_server, seen) = fake_app(&pipe, true);
    let got = wait_for(&seen, 1, Duration::from_secs(12));
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].1["event"], json!("done"));
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(seen.lock().unwrap().len(), 1, "busy was not delivered later");
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// keymotion-cli copied into `dir\bin`, as an installed copy sits.
fn installed_copy(dir: &Path) -> PathBuf {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(dir.join("keymotion.exe"), b"").unwrap();
    let exe = bin.join("keymotion-cli.exe");
    std::fs::copy(EXE, &exe).unwrap();
    exe
}

fn powershell() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into()))
        .join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
}

/// Run a Codex hook line as Codex 0.166.0 does on Windows: its shell,
/// PowerShell, `-NoProfile -Command <line>`, the event on stdin.
fn run_codex_line(line: &str, input: &[u8], pipe: &str) -> Run {
    let r = run_program(&powershell(), &["-NoProfile", "-Command", line], input, pipe, None);
    assert_eq!(r.code, 0, "{line}: {r:?}");
    assert!(r.stdout.is_empty(), "{line}: {r:?}");
    r
}

#[test]
fn connect_writes_hook_lines_that_run_from_a_folder_with_spaces() {
    let home = temp_home();
    let exe = installed_copy(&home.join("Program Files").join("Key Motion"));
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let users = "{\n  \"model\": \"opus\",\n  \"hooks\": {\n    \"Stop\": [\n      {\n        \"hooks\": [\n          {\n            \"type\": \"command\",\n            \"command\": \"say done\"\n          }\n        ]\n      }\n    ]\n  }\n}\n";
    std::fs::write(home.join(".claude").join("settings.json"), users).unwrap();
    let pipe = unique_pipe();
    let cli = |args: &[&str]| run_program(&exe, args, b"", &pipe, Some(&home));

    // Not at a terminal: it asks for --yes and changes nothing.
    let r = cli(&["connect", "claude-code"]);
    assert_eq!(r.code, 64, "{r:?}");
    let r = cli(&["connect", "claude-code", "--dry-run", "--json"]);
    assert_eq!(r.code, 0, "{r:?}");
    assert_eq!(read(&home.join(".claude").join("settings.json")), users);
    let r = cli(&["connect", "claude-code", "--yes", "--json"]);
    assert_eq!(r.code, 0, "{r:?}");
    let r = cli(&["connect", "codex", "--yes"]);
    assert_eq!(r.code, 0, "{r:?}");
    let v: Value = serde_json::from_slice(&cli(&["connections", "--json"]).stdout).unwrap();
    let states: Vec<&str> =
        v["data"]["agents"].as_array().unwrap().iter().map(|a| a["state"].as_str().unwrap()).collect();
    assert_eq!(states, ["connected", "needs_approval"], "{v}");
    assert_eq!(v["data"]["cli"]["kind"], json!("installed"));

    let (_server, seen) = fake_app(&pipe, true);
    // Claude Code: the program and its arguments, no shell.
    let settings: Value = serde_json::from_str(&read(&home.join(".claude").join("settings.json"))).unwrap();
    let ours = &settings["hooks"]["Stop"][1]["hooks"][0];
    assert_eq!(ours["command"], json!(exe.display().to_string()));
    let args: Vec<&str> = ours["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
    let r = run_program(Path::new(ours["command"].as_str().unwrap()), &args, STOP, &pipe, None);
    assert_eq!((r.code, r.stdout.len()), (0, 0), "{r:?}");
    // Codex: one line for PowerShell.
    let hooks: Value = serde_json::from_str(&read(&home.join(".codex").join("hooks.json"))).unwrap();
    let line = hooks["hooks"]["Stop"][0]["hooks"][0]["command"].as_str().unwrap().to_string();
    assert!(line.starts_with("& '") && line.contains("Key Motion"), "{line}");
    let codex_stop = br#"{"session_id":"thr_1","cwd":"C:\\w","hook_event_name":"Stop","turn_id":"t","model":"m"}"#;
    run_codex_line(&line, codex_stop, &pipe);
    let got = wait_for(&seen, 2, Duration::from_secs(5));
    let agents: Vec<&str> = got.iter().map(|(_, a)| a["agent"].as_str().unwrap()).collect();
    assert_eq!(agents, ["claude-code", "codex"], "{got:?}");

    // Disconnect: the user's file as it was; our files gone.
    assert_eq!(cli(&["disconnect", "claude-code", "--yes"]).code, 0);
    assert_eq!(cli(&["disconnect", "codex", "--yes"]).code, 0);
    assert_eq!(read(&home.join(".claude").join("settings.json")), users);
    assert!(!home.join(".codex").join("hooks.json").exists());
    assert!(!home.join(".claude").join("skills").join("keymotion").exists());
    assert!(!home.join(".agents").join("skills").join("keymotion").exists());
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_path_without_spaces_runs_plain_in_powershell_and_cmd() {
    // Installed where the path needs no quotes: the line is the bare path,
    // which PowerShell and cmd.exe (Codex's shell when it knows none) both run.
    let dir = std::env::temp_dir().join(unique("km-plain-"));
    let exe = installed_copy(&dir);
    let home = temp_home();
    std::fs::create_dir_all(home.join(".codex")).unwrap();
    let pipe = unique_pipe();
    let r = run_program(&exe, &["connect", "codex", "--yes"], b"", &pipe, Some(&home));
    assert_eq!(r.code, 0, "{r:?}");
    let hooks: Value = serde_json::from_str(&read(&home.join(".codex").join("hooks.json"))).unwrap();
    let line = hooks["hooks"]["Stop"][0]["hooks"][0]["command"].as_str().unwrap().to_string();
    assert_eq!(line, format!("{} hook codex", exe.display()));
    let (_server, seen) = fake_app(&pipe, true);
    let stop = br#"{"session_id":"thr_2","cwd":"C:\\w","hook_event_name":"Stop","turn_id":"t","model":"m"}"#;
    run_codex_line(&line, stop, &pipe);
    let cmd =
        PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into())).join(r"System32\cmd.exe");
    let r = run_program(&cmd, &["/C", &line], stop, &pipe, None);
    assert_eq!((r.code, r.stdout.len()), (0, 0), "{r:?}");
    assert_eq!(wait_for(&seen, 2, Duration::from_secs(5)).len(), 2);
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&dir);
}
