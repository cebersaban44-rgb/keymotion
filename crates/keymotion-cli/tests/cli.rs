//! The real `keymotion-cli` program against a fake Keymotion: kb-agent's
//! own pipe server with a handler that answers like the app, on a pipe of
//! its own (`KEYMOTION_CLI_TEST_PIPE`, which also keeps the program from
//! starting any real Keymotion). No keyboard, no app.

#![cfg(windows)]

use std::process::{Command as Process, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use kb_agent::transport::serve;
use kb_agent::{AgentError, Channel, Command, ErrorCode, ServerInfo};
use kb_winsys::pipe::{Incoming, PipeServer};
use serde_json::{json, Value};

const EXE: &str = env!("CARGO_BIN_EXE_keymotion-cli");

fn unique_pipe() -> String {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    format!(r"\\.\pipe\keymotion-cli-test-{}-{}-{nanos}", std::process::id(), COUNT.fetch_add(1, Ordering::Relaxed))
}

#[derive(Debug)]
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
    took: Duration,
}

impl Run {
    /// The one JSON object on stdout.
    fn json(&self) -> Value {
        let lines: Vec<&str> = self.stdout.lines().collect();
        assert_eq!(lines.len(), 1, "exactly one line of JSON: {self:?}");
        serde_json::from_str(lines[0]).unwrap_or_else(|e| panic!("not JSON ({e}): {self:?}"))
    }
}

fn run_with(pipe: &str, args: &[&str], env: &[(&str, &str)]) -> Run {
    let started = Instant::now();
    let mut cmd = Process::new(EXE);
    cmd.args(args)
        .env("KEYMOTION_CLI_TEST_PIPE", pipe)
        .env_remove("KEYMOTION_OUTPUT")
        .env_remove("KEYMOTION_SOURCE")
        .stdin(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("keymotion-cli runs");
    let run = Run {
        code: out.status.code().expect("an exit code"),
        stdout: String::from_utf8(out.stdout).expect("UTF-8 stdout"),
        stderr: String::from_utf8(out.stderr).expect("UTF-8 stderr"),
        took: started.elapsed(),
    };
    assert_ne!(run.code, 2, "exit code 2 is never used: {run:?}");
    run
}

fn run(pipe: &str, args: &[&str]) -> Run {
    run_with(pipe, args, &[])
}

/// What the fake app saw and how it answers.
#[derive(Default)]
struct Fake {
    access: AtomicBool,
    /// (source, command, args) of every request after hello.
    seen: Mutex<Vec<(String, String, Value)>>,
}

impl Fake {
    fn last(&self) -> (String, String, Value) {
        self.seen.lock().unwrap().last().cloned().expect("a request")
    }
}

fn info() -> ServerInfo {
    ServerInfo { version: env!("CARGO_PKG_VERSION").into(), channel: Channel::Dev }
}

/// A fake Keymotion on `pipe`, answering like the app's agent module.
fn fake_app(pipe: &str) -> (PipeServer, Arc<Fake>) {
    let fake = Arc::new(Fake::default());
    let me = fake.clone();
    let server = serve(pipe, info(), move |call, command| {
        let args = Value::Object(command.args());
        me.seen.lock().unwrap().push((call.source.as_str().to_string(), command.name().to_string(), args.clone()));
        let source = call.source.as_str();
        let access = me.access.load(Ordering::SeqCst);
        let lit = |id: String, ms: u64| {
            json!({"id": id, "shown": true, "lit": true, "expires_in_ms": ms, "replaced": false, "evicted": [], "coalesced": false, "hidden": null})
        };
        match command {
            Command::Status(_) => Ok(json!({
                "app": {"running": true, "version": env!("CARGO_PKG_VERSION"), "channel": "dev", "protocol": call.peer.protocol},
                "agents": {"enabled": access, "screen": false, "focus": false, "profiles": false, "agents_may_edit": true, "hide_in_game": true},
                "keyboards": [{"model": "ajazz-ak980-pro", "name": "AJAZZ AK980 PRO", "connection": "usb", "battery": null, "lights": true, "screen": true}],
                "lights": "shown", "quiet": "off", "focus": null, "signals": [],
                "screen_budget": null, "flash_write": null, "conflict": null,
            })),
            Command::Keys(_) => Ok(json!({
                "model": "ajazz-ak980-pro", "name": "AJAZZ AK980 PRO",
                "keys": [{"id": "esc", "label": "Esc", "x": 0.0, "y": 0.0, "w": 1.0, "h": 1.0, "groups": []},
                         {"id": "f1", "label": "F1", "x": 1.5, "y": 0.0, "w": 1.0, "h": 1.0, "groups": ["f-row"]}],
                "groups": [{"name": "all", "keys": []}, {"name": "f-row", "keys": ["f1"]}],
            })),
            Command::Log(a) => Ok(json!({"entries": [{"seq": 1, "at_ms": 0.0, "source": "cli", "command": "signal", "result": "ok", "limit": a.validate()?}]})),
            Command::SettingsGet(a) => {
                a.validate()?;
                Ok(json!({"settings": {"signals.done.color": "#00ff00"}, "agents_may_edit": true, "user_only": ["enabled"]}))
            }
            _ if !access => Err(AgentError::new(ErrorCode::AgentAccessOff, "Agent access is off in Keymotion.")
                .with_hint("The user can turn it on in Keymotion › Tools › AI agents.")),
            Command::Signal(a) => {
                let s = a.validate()?;
                Ok(lit(a.id.clone().unwrap_or_else(|| format!("{source}:{}", s.name.as_str())), 120_000))
            }
            Command::Light(a) => {
                a.validate()?;
                Ok(lit(a.id.clone().unwrap_or_else(|| format!("{source}:light")), 60_000))
            }
            Command::Progress(a) => {
                let p = a.validate()?;
                let mut v = lit(a.id.clone().unwrap_or_else(|| format!("{source}:progress")), 600_000);
                v["percent"] = json!(p.percent);
                Ok(v)
            }
            Command::Clear(a) => {
                a.validate()?;
                Ok(json!({"cleared": 1}))
            }
            Command::SettingsSet(a) => {
                let pairs = a.validate()?;
                let (key, value) = pairs[0].clone();
                if key == "enabled" {
                    return Err(AgentError::new(ErrorCode::PermissionOff, "Only the user can change 'enabled', in Keymotion."));
                }
                Ok(json!({"changed": [{"key": key, "before": "#00ff00", "after": value}], "log_entry": 7}))
            }
            Command::Hello(_) => Err(AgentError::new(ErrorCode::Internal, "hello reached the handler")),
        }
    })
    .expect("the fake app serves its pipe");
    (server, fake)
}

#[test]
fn every_command_reaches_the_app_and_prints_one_json_object() {
    let pipe = unique_pipe();
    let (_server, fake) = fake_app(&pipe);
    fake.access.store(true, Ordering::SeqCst);

    let r = run(&pipe, &["status", "--json"]);
    assert_eq!(r.code, 0, "{r:?}");
    let v = r.json();
    assert_eq!((v["ok"].clone(), v["schema"].clone(), v["command"].clone()), (json!(true), json!(1), json!("status")));
    assert_eq!(v["data"]["app"]["running"], json!(true));
    assert_eq!(v["warnings"], json!([]));
    assert!(r.stdout.starts_with(r#"{"ok":true,"schema":1,"command":"status","data":"#), "{}", r.stdout);

    let r = run(&pipe, &["keys", "--json"]);
    assert_eq!((r.code, r.json()["data"]["model"].clone()), (0, json!("ajazz-ak980-pro")));

    let r =
        run(&pipe, &["--source", "claude-code:3f9a", "signal", "done", "--color", "amber", "--for", "5m", "--json"]);
    assert_eq!(r.code, 0, "{r:?}");
    assert_eq!(r.json()["data"]["id"], json!("claude-code:3f9a:done"));
    assert_eq!(
        fake.last(),
        ("claude-code:3f9a".into(), "signal".into(), json!({"name": "done", "color": "amber", "for_ms": 300_000}))
    );

    let r = run_with(
        &pipe,
        &["light", "--keys", "f1-f12", "--color", "green", "--effect", "wave", "--times", "1"],
        &[("KEYMOTION_SOURCE", "codex"), ("KEYMOTION_OUTPUT", "json")],
    );
    assert_eq!((r.code, r.json()["data"]["id"].clone()), (0, json!("codex:light")), "{r:?}");
    assert_eq!(fake.last().2, json!({"keys": "f1-f12", "color": "green", "effect": "wave", "times": 1}));

    let r = run(&pipe, &["progress", "40%", "--label", "build", "--json"]);
    assert_eq!((r.code, r.json()["data"]["percent"].clone()), (0, json!(40.0)), "{r:?}");
    assert_eq!(fake.last().2, json!({"percent": 40.0, "label": "build"}));

    let r = run(&pipe, &["clear", "--id", "wait", "--json"]);
    assert_eq!((r.code, r.json()["data"]["cleared"].clone()), (0, json!(1)));
    assert_eq!(fake.last().2, json!({"id": "wait"}));
    let r = run(&pipe, &["clear", "--json"]);
    assert_eq!((r.code, fake.last().2), (0, json!({})));

    let r = run(&pipe, &["log", "--limit", "5", "--json"]);
    assert_eq!((r.code, r.json()["data"]["entries"][0]["limit"].clone()), (0, json!(5)));

    let r = run(&pipe, &["settings", "get", "signals.done", "--json"]);
    assert_eq!((r.code, r.json()["command"].clone()), (0, json!("settings get")));
    assert_eq!(fake.last(), ("cli".into(), "settings_get".into(), json!({"key": "signals.done"})));

    let r = run(&pipe, &["settings", "set", "signals.done.color", "blue", "--json"]);
    assert_eq!((r.code, r.json()["data"]["changed"][0]["after"].clone()), (0, json!("blue")), "{r:?}");
    assert_eq!(fake.last().2, json!({"values": {"signals.done.color": "blue"}}));

    // The protected keys are the app's to refuse: 77.
    let r = run(&pipe, &["settings", "set", "enabled", "true", "--json"]);
    assert_eq!((r.code, r.json()["error"]["code"].clone()), (77, json!("permission_off")), "{r:?}");
    assert_eq!(fake.last().2, json!({"values": {"enabled": true}}));

    let r = run(&pipe, &["version", "--json"]);
    let v = r.json();
    assert_eq!(r.code, 0);
    assert_eq!(v["data"]["cli"]["version"], json!(env!("CARGO_PKG_VERSION")));
    assert_eq!(
        (v["data"]["app"]["running"].clone(), v["data"]["app"]["compatible"].clone()),
        (json!(true), json!(true))
    );

    let r = run(&pipe, &["start", "--json"]);
    assert_eq!((r.code, r.json()["data"]["already_running"].clone()), (0, json!(true)), "{r:?}");
}

#[test]
fn human_output_is_short_and_errors_go_to_stderr() {
    let pipe = unique_pipe();
    let (_server, fake) = fake_app(&pipe);
    fake.access.store(true, Ordering::SeqCst);
    let r = run(&pipe, &["signal", "done"]);
    assert_eq!((r.code, r.stdout.as_str(), r.stderr.as_str()), (0, "Signal cli:done on for 2m.\n", ""));
    let r = run(&pipe, &["signal", "done", "-q"]);
    assert_eq!((r.code, r.stdout.as_str(), r.stderr.as_str()), (0, "", ""));
    let r = run(&pipe, &["status"]);
    assert!(r.stdout.contains("Agent access: on") && r.stdout.contains("AJAZZ AK980 PRO"), "{r:?}");
    fake.access.store(false, Ordering::SeqCst);
    let r = run(&pipe, &["light", "--color", "red"]);
    assert_eq!(r.code, 77);
    assert_eq!(r.stdout, "");
    assert!(r.stderr.starts_with("error: Agent access is off in Keymotion.\nhint: "), "{r:?}");
}

#[test]
fn access_off_refuses_lights_with_77_but_reading_works() {
    let pipe = unique_pipe();
    let (_server, _fake) = fake_app(&pipe);
    for args in [
        &["signal", "attention", "--json"][..],
        &["light", "--color", "red", "--json"],
        &["progress", "10", "--json"],
        &["clear", "--json"],
    ] {
        let r = run(&pipe, args);
        assert_eq!(r.code, 77, "{args:?}: {r:?}");
        let v = r.json();
        assert_eq!((v["ok"].clone(), v["error"]["code"].clone()), (json!(false), json!("agent_access_off")));
        assert!(v["error"]["hint"].as_str().is_some_and(|h| h.contains("AI agents")), "{v}");
        assert_eq!(v["error"]["retry_after_ms"], Value::Null);
    }
    let r = run(&pipe, &["status", "--json"]);
    assert_eq!(r.code, 0);
    let v = r.json();
    assert_eq!(v["data"]["agents"]["enabled"], json!(false));
    assert!(v["warnings"][0].as_str().is_some_and(|w| w.contains("Agent access is off")), "{v}");
    assert_eq!(run(&pipe, &["keys", "--json"]).code, 0);
}

#[test]
fn without_the_app_status_says_so_and_no_start_gives_69_at_once() {
    let pipe = unique_pipe();
    let r = run(&pipe, &["status", "--json"]);
    assert_eq!((r.code, r.json()["data"].clone()), (0, json!({"app": {"running": false}})), "{r:?}");
    let r = run(&pipe, &["status"]);
    assert_eq!((r.code, r.stdout.as_str()), (0, "Keymotion is not running.\n"));
    let r = run(&pipe, &["version", "--json"]);
    assert_eq!((r.code, r.json()["data"]["app"].clone()), (0, json!({"running": false})));
    for args in [
        &["signal", "done", "--no-start", "--json"][..],
        &["--no-start", "light", "--color", "red", "--json"],
        &["keys", "--no-start", "--json"],
        &["settings", "get", "--no-start", "--json"],
    ] {
        let r = run(&pipe, args);
        assert_eq!(r.code, 69, "{args:?}: {r:?}");
        assert_eq!(r.json()["error"]["code"], json!("app_not_running"));
        assert!(r.took < Duration::from_secs(3), "{args:?} took {:?}", r.took);
    }
    // A value is checked before anything is sent: 65 with or without the app.
    let r = run(&pipe, &["light", "--color", "teal", "--json"]);
    assert_eq!((r.code, r.json()["error"]["code"].clone()), (65, json!("bad_color")));
    let r = run(&pipe, &["status", "--source", "Not Valid!", "--json"]);
    assert_eq!((r.code, r.json()["error"]["code"].clone()), (65, json!("bad_source")));
    let r = run(&pipe, &["status", "--timeout", "soon", "--json"]);
    assert_eq!((r.code, r.json()["error"]["code"].clone()), (65, json!("bad_duration")));
}

#[test]
fn a_command_waits_for_an_app_that_is_starting() {
    // The test pipe starts nothing; the fake app comes up a moment later,
    // as a real one would after the start.
    let pipe = unique_pipe();
    let later = pipe.clone();
    let server = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(800));
        let (server, fake) = fake_app(&later);
        fake.access.store(true, Ordering::SeqCst);
        std::thread::sleep(Duration::from_secs(6));
        drop(server);
    });
    let r = run(&pipe, &["signal", "done", "--json", "--log", "info"]);
    assert_eq!(r.code, 0, "{r:?}");
    let v = r.json();
    assert!(v["warnings"][0].as_str().is_some_and(|w| w.contains("started it in the background")), "{v}");
    assert!(r.stderr.contains("starting Keymotion: nothing (a test)"), "{r:?}");
    assert!(r.took >= Duration::from_millis(700) && r.took < Duration::from_secs(6), "{:?}", r.took);
    server.join().unwrap();
}

#[test]
fn start_starts_and_reports_how() {
    let pipe = unique_pipe();
    let later = pipe.clone();
    let server = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(500));
        let (server, _) = fake_app(&later);
        std::thread::sleep(Duration::from_secs(5));
        drop(server);
    });
    let r = run(&pipe, &["start", "--json"]);
    assert_eq!(r.code, 0, "{r:?}");
    let data = r.json()["data"].clone();
    assert_eq!((data["already_running"].clone(), data["method"].clone()), (json!(false), json!("test")));
    assert!(data["waited_ms"].as_u64().is_some_and(|ms| ms >= 400), "{data}");
    let r = run(&pipe, &["start", "--no-start", "--json"]);
    assert_eq!(r.code, 64, "{r:?}");
    server.join().unwrap();
}

#[test]
fn a_hung_app_times_out_with_75_within_the_timeout() {
    let pipe = unique_pipe();
    let _server = serve(&pipe, info(), |_, _| {
        std::thread::sleep(Duration::from_secs(4));
        Ok(json!({}))
    })
    .unwrap();
    let r = run(&pipe, &["status", "--timeout", "600ms", "--json"]);
    assert_eq!((r.code, r.json()["error"]["code"].clone()), (75, json!("timeout")), "{r:?}");
    assert!(r.took < Duration::from_millis(2500), "{:?}", r.took);

    // One deadline for connecting and the call: an app that never answers
    // hello is a timeout too, within the same time.
    let pipe = unique_pipe();
    let _silent = kb_winsys::pipe::serve(&pipe, || {
        |_: Incoming<'_>| {
            std::thread::sleep(Duration::from_secs(4));
            String::new()
        }
    })
    .unwrap();
    let r = run(&pipe, &["signal", "done", "--timeout", "600ms", "--json"]);
    assert_eq!((r.code, r.json()["error"]["code"].clone()), (75, json!("timeout")), "{r:?}");
    assert!(r.took < Duration::from_millis(2500), "{:?}", r.took);
}

#[test]
fn another_protocol_is_76() {
    // A newer app that refuses this protocol, as kb-agent's hello does.
    let pipe = unique_pipe();
    let _refuses = kb_winsys::pipe::serve(&pipe, || {
        |_: Incoming<'_>| {
            r#"{"v":1,"id":"1","ok":false,"error":{"code":"protocol_mismatch","message":"keymotion-cli speaks protocol 1–1, Keymotion speaks 2–2.","hint":"This keymotion-cli is older than Keymotion. Use the one installed with Keymotion."}}"#.to_string()
        }
    })
    .unwrap();
    let r = run(&pipe, &["signal", "done", "--json"]);
    assert_eq!((r.code, r.json()["error"]["code"].clone()), (76, json!("protocol_mismatch")), "{r:?}");
    let r = run(&pipe, &["status", "--json"]);
    assert_eq!(r.code, 76, "{r:?}");
    // `version` still reports both sides.
    let r = run(&pipe, &["version", "--json"]);
    assert_eq!(r.code, 0, "{r:?}");
    assert_eq!(r.json()["data"]["app"]["compatible"], json!(false));

    // One that picks a version this CLI does not speak.
    let pipe = unique_pipe();
    let _picks = kb_winsys::pipe::serve(&pipe, || {
        |_: Incoming<'_>| {
            r#"{"v":1,"id":"1","ok":true,"data":{"app":"keymotion","version":"9.0.0","channel":"store","protocol":2}}"#
                .to_string()
        }
    })
    .unwrap();
    let r = run(&pipe, &["light", "--color", "red", "--json"]);
    assert_eq!((r.code, r.json()["error"]["code"].clone()), (76, json!("protocol_mismatch")), "{r:?}");
}

#[test]
fn usage_errors_are_64_never_2() {
    let pipe = unique_pipe();
    for line in [
        "",
        "sparkle",
        "--sparkle",
        "light --colour red",
        "signal",
        "progress",
        "settings",
        "settings set enabled",
        "light --color red --times 2 --for 5s",
        "log --limit many",
        "--log loud status",
        "status extra",
        "hook claude-code",
    ] {
        let args: Vec<&str> = line.split_whitespace().collect();
        let r = run(&pipe, &args);
        assert_eq!(r.code, 64, "{line:?}: {r:?}");
        assert_eq!(r.stdout, "", "{line:?}: human mode prints usage errors on stderr");
        assert!(!r.stderr.is_empty(), "{line:?}");

        let mut with_json = args.clone();
        with_json.push("--json");
        let r = run(&pipe, &with_json);
        assert_eq!(r.code, 64, "{line:?} --json: {r:?}");
        let v = r.json();
        assert_eq!((v["ok"].clone(), v["error"]["code"].clone()), (json!(false), json!("bad_argument")), "{line:?}");
        assert!(v["error"]["hint"].as_str().is_some_and(|h| !h.is_empty()));
    }
    for line in ["--help", "--version", "signal --help", "help"] {
        let args: Vec<&str> = line.split_whitespace().collect();
        let r = run(&pipe, &args);
        assert_eq!(r.code, 0, "{line}: {r:?}");
        assert!(!r.stdout.is_empty(), "{line}");
    }
    assert_eq!(
        run(&pipe, &["--version"]).stdout,
        format!("keymotion-cli {} (protocol 1)\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn guide_and_doctor_work_without_the_app() {
    let pipe = unique_pipe();
    let r = run(&pipe, &["guide"]);
    assert_eq!(r.code, 0);
    assert!(r.stdout.lines().count() <= 60 && r.stdout.contains("You decide how to tell the user things"));
    let r = run(&pipe, &["guide", "--json"]);
    assert!(r.json()["data"]["text"].as_str().is_some_and(|t| t.contains("keymotion-cli signal done")));

    let r = run(&pipe, &["doctor", "--json"]);
    assert_eq!(r.code, 0, "{r:?}");
    let v = r.json();
    let checks = v["data"]["checks"].as_array().unwrap();
    let check = |id: &str| checks.iter().find(|c| c["id"] == json!(id)).cloned().unwrap_or_else(|| panic!("{id}: {v}"));
    assert_eq!(check("cli")["status"], json!("ok"));
    assert_eq!(
        (check("app")["status"].clone(), check("app")["detail"].clone()),
        (json!("warn"), json!("Keymotion is not running"))
    );
    assert_eq!(
        check("start")["detail"],
        json!("when Keymotion is not running, commands start it through nothing (a test)")
    );
    assert!(checks.iter().any(|c| c["id"] == json!("box")), "{v}");

    // With the app: its pipe is served by this test process, not keymotion.exe.
    let (_server, fake) = fake_app(&pipe);
    fake.access.store(true, Ordering::SeqCst);
    let v = run(&pipe, &["doctor", "--json"]).json();
    let checks = v["data"]["checks"].as_array().unwrap().clone();
    let check = |id: &str| checks.iter().find(|c| c["id"] == json!(id)).cloned().unwrap_or_else(|| panic!("{id}: {v}"));
    assert_eq!(check("app")["status"], json!("ok"));
    assert_eq!(check("pipe")["status"], json!("warn"), "{v}");
    assert!(check("pipe")["detail"].as_str().unwrap().contains(&format!("process {}", std::process::id())));
    assert_eq!(check("owner")["status"], json!("ok"));
    assert_eq!(check("versions")["status"], json!("ok"));
    assert_eq!(check("access")["status"], json!("ok"));
    assert_eq!(check("keyboard")["status"], json!("ok"));
}
