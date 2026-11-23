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
no setup. Only the user turns on agent access and each permission, in Keymotion.

First
  keymotion-cli status --json       running? access on? keyboard? your lights
  keymotion-cli keys --json         this keyboard's key ids, rows and groups
With --json, stdout is one JSON object: {"ok":true,"data":{...},"warnings":[]}
or {"ok":false,"error":{"code":"...","message":"...","hint":"..."}}.

Ready signals (the user picks the look; --color --effect --keys --for override)
  keymotion-cli signal done         also attention, error, info, busy

Your own lights (ideas, not rules)
  keymotion-cli light --keys 1-3 --color red --for 10m
      "3 problems": the keys 1, 2 and 3 in red
  keymotion-cli light --keys f1-f12 --color green --effect wave --times 1
      "tests passed": one green wave across the F row
  keymotion-cli light --keys all --color amber --effect pulse --for 30m --id ask
      a long, calm pulse while you wait for the user
  keymotion-cli spell "DONE" --color green
      a word on its keys, letter after letter (--speed 1-5, --times N)
  keymotion-cli scene s.json --repeat 2 --save qw
      your own timeline; s.json: [{"keys":"q","color":"red","for":"400ms"},
      {"keys":"w","color":"blue","effect":"pulse"}]. Later: pattern play qw
  keymotion-cli progress 40 --label build
      a bar on the F row, 0 to 100; send it again as work goes on
  keymotion-cli clear --id ask      that light off; plain clear: all of yours

When the user allowed it (77 otherwise: tell the user, do not retry)
  keymotion-cli focus start --for 50m
      a 50-minute focus block on the user's timer; pause, skip, stop, status
  keymotion-cli profile show "Night Owl" --for 30s
      a profile's lights for a while; profile list; apply NAME --write-flash
  keymotion-cli card "Deployed\n4 min" --title CI --write-flash
      a card on the screen; --image FILE (PNG/JPEG/GIF/WebP, 40 KiB, still).
      Each card writes the screen's memory: a few an hour, for real news.

Values
  Keys: ids (esc, f1, w, 1), ranges (f1-f12, 1-3), groups (all, f-row, numbers,
    letters, wasd, arrows, numpad), comma-separated. keys lists them.
  Colors: red green blue amber yellow orange purple pink cyan white, or #RRGGBB.
  Effects: solid pulse blink flash wave (--direction left|right|up|down);
    --speed 1-5. Blinking stays at 3 flashes a second or less.
  Time: --times N runs it N times; --for 500ms|30s|5m|1h (light 60s, bar 10m).
  Ids: --id keeps up to 8 lights apart; you can clear only your own lights.

Connected coding apps light done, waiting and errors themselves; add the rest.
More: log, settings get|set, doctor. Keymotion starts unseen (--no-start: not).

Exit codes
  0 done  1 unexpected  64 bad command line  65 bad value (color, key, time)
  69 not available: no app, no keyboard, or it needs the USB cable
  75 try again later (rate limit or timeout; see retry_after_ms)
  76 Keymotion and keymotion-cli differ: update the older one
  77 not allowed (access or a permission is off): tell the user, do not retry
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
                // A scene's file is the agent's: the JSON is checked below.
                Parsed::Run(inv) if command.starts_with("scene ") => assert_eq!(inv.command.name(), "scene"),
                Parsed::Run(inv) => {
                    if let Err(e) = inv.command.plan() {
                        panic!("{command}: {e:?}");
                    }
                }
                other => panic!("{command}: {other:?}"),
            }
        }
        // The scene the guide shows, its two lines joined.
        let at = GUIDE.find("[{").unwrap();
        let json: String = GUIDE[at..GUIDE.find("}]").unwrap() + 2].split_whitespace().collect();
        let scene = crate::cli::scene_from_json(&serde_json::from_str(&json).unwrap()).unwrap();
        assert_eq!(scene.validate().unwrap().steps.len(), 2, "{json}");
    }

    #[test]
    fn it_says_what_the_owner_asked_for() {
        // The agent chooses; ready signals are optional; examples are
        // creative, not only `signal done`.
        assert!(GUIDE.contains("You decide how to tell the user things"));
        assert!(GUIDE.contains("optional"));
        for idea in [
            "--keys 1-3 --color red",
            "--effect wave --times 1",
            "progress 40",
            "--effect pulse --for 30m",
            "focus start --for 50m",
            "profile show",
            "card \"Deployed",
            "spell \"DONE\"",
            "scene s.json",
            "pattern play",
        ] {
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
