//! `keymotion-cli guide`: what an agent reads before its first use. At most
//! 60 lines (about 600 tokens), so it does not crowd an agent's context;
//! the tests count them and run every example through the parser.
//!
//! The owner's principle (plan §12.5): the agent says what it wants, the way
//! it wants; the ready signals are shortcuts, not the language. So the
//! examples show keys, colors and effects used freely, and only commands
//! this build has.

pub const GUIDE: &str = r#"keymotion-cli: light the keys of the user's keyboard through the Keymotion app.
Lights live in the keyboard's working memory only: nothing is saved, and each
light ends by itself (2h at most), or when you clear it.

You decide how to tell the user things. The ready signals below are optional
shortcuts; you may as well pick keys, colors, effects and timing yourself, with
no setup. The user's only switch: Keymotion > Tools > AI agents > agent access.

First
  keymotion-cli status --json       running? access on? keyboard? your lights
  keymotion-cli keys --json         this keyboard's key ids, rows and groups
With --json, stdout is one JSON object: {"ok":true,"data":{...},"warnings":[]}
or {"ok":false,"error":{"code":"...","message":"...","hint":"..."}}.

Ready signals (the user picks the look; --color --effect --keys --for override)
  keymotion-cli signal done         a task finished
  keymotion-cli signal attention    you need the user: flashes, then pulses
  keymotion-cli signal error        something failed
  keymotion-cli signal info         a short note
  keymotion-cli signal busy         working: clears your earlier signals

Your own lights (ideas, not rules)
  keymotion-cli light --keys 1-3 --color red --for 10m
      "3 problems": the keys 1, 2 and 3 in red
  keymotion-cli light --keys f1-f12 --color green --effect wave --times 1
      "tests passed": one green wave across the F row
  keymotion-cli light --keys all --color amber --effect pulse --for 30m --id ask
      a long, calm pulse while you wait for the user
  keymotion-cli light --keys o,k --color cyan --effect blink --times 3
      a word on its letter keys
  keymotion-cli progress 40 --label build
      a bar on the F row, 0 to 100; send it again as work goes on
  keymotion-cli clear --id ask      that light off; plain clear: all of yours

Values
  Keys: ids (esc, f1, w, 1), ranges (f1-f12, 1-3), groups (all, f-row, numbers,
    letters, wasd, arrows, numpad), comma-separated. keys lists them.
  Colors: red green blue amber yellow orange purple pink cyan white, or #RRGGBB.
  Effects: solid pulse blink flash wave (--direction left|right|up|down);
    --speed 1-5. Blinking stays at 3 flashes a second or less.
  Time: --times N runs the effect N times; --for 500ms|30s|5m|1h. Unless told,
    a light lasts 60s and a bar 10m after its last update.
  Ids: without --id, a command replaces your previous one of its kind (signal
    done replaces signal done); give ids to keep up to 8 lights. --source NAME
    (or KEYMOTION_SOURCE) names you; you can clear only your own lights.

Your coding app may be connected (keymotion-cli connections): then "done",
"waiting for you" and errors light by themselves. Add what they cannot say.
More: log, settings get, settings set KEY VALUE (looks and behaviour; the user
can undo it), doctor (setup problems), start. If Keymotion is not running,
commands start it with no window (--no-start: they do not).

Exit codes
  0 done    64 bad command line    65 bad value (color, key, time)
  69 not available: no app, no keyboard, or it needs the USB cable
  75 try again later (rate limit or timeout; see retry_after_ms)
  76 Keymotion and keymotion-cli differ: update the older one
  77 not allowed (agent access off, a setting only the user may change):
     tell the user, do not retry      1 unexpected
"#;

pub fn data() -> serde_json::Value {
    serde_json::json!({ "text": GUIDE, "lines": GUIDE.lines().count() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{parse, Env, Parsed};

    #[test]
    fn the_guide_stays_short() {
        let lines = GUIDE.lines().count();
        assert!(lines <= 60, "the guide has {lines} lines; keep it at 60 or fewer");
        for line in GUIDE.lines() {
            assert!(line.chars().count() <= 80, "too wide: {line}");
            assert!(line.is_ascii(), "plain ASCII, any console shows it: {line}");
        }
    }

    #[test]
    fn every_example_is_a_command_this_build_takes() {
        let examples: Vec<&str> = GUIDE.lines().filter_map(|l| l.trim_start().strip_prefix("keymotion-cli ")).collect();
        assert!(examples.len() >= 10, "{examples:?}");
        for example in examples {
            // The command, up to the explanation after two spaces.
            let command = example.split("  ").next().unwrap_or(example);
            let words = shell_words(command);
            let args = std::iter::once("keymotion-cli".to_string()).chain(words);
            match parse(args, &Env::default()) {
                Parsed::Run(inv) => {
                    if let Err(e) = inv.command.plan() {
                        panic!("{command}: {e:?}");
                    }
                }
                other => panic!("{command}: {other:?}"),
            }
        }
    }

    #[test]
    fn it_says_what_the_owner_asked_for() {
        // The agent chooses; ready signals are optional; examples are
        // creative, not only `signal done`.
        assert!(GUIDE.contains("You decide how to tell the user things"));
        assert!(GUIDE.contains("optional"));
        for idea in ["--keys 1-3 --color red", "--effect wave --times 1", "progress 40", "--effect pulse --for 30m"] {
            assert!(GUIDE.contains(idea), "{idea}");
        }
        for code in ["0 done", "64", "65", "69", "75", "76", "77"] {
            assert!(GUIDE.contains(code), "{code}");
        }
        assert!(GUIDE.contains("--json"));
    }

    /// Words split at spaces, double quotes kept together.
    fn shell_words(line: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut word = String::new();
        let mut quoted = false;
        for c in line.chars() {
            match c {
                '"' => quoted = !quoted,
                ' ' if !quoted => {
                    if !word.is_empty() {
                        words.push(std::mem::take(&mut word));
                    }
                }
                c => word.push(c),
            }
        }
        if !word.is_empty() {
            words.push(word);
        }
        words
    }
}
