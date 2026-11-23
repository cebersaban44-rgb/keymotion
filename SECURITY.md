# Security

Keymotion runs on your computer, talks to your keyboard over USB and Bluetooth, reads the calendar addresses you give it, and takes requests from the AI coding agents you connect. If you find a way to misuse any of that, please tell us privately first, so it can be fixed before it's public.

*Türkçe: güvenlik sorunlarını aşağıdaki gizli yolla bildirin; Türkçe yazabilirsiniz.*

## How to report

Use GitHub's private reporting: open the repository's **Security** tab and choose **Report a vulnerability**, or go straight to [the private report form](https://github.com/cebersaban44-rgb/keymotion/security/advisories/new). Only you and the maintainer see the report.

Please don't open a public issue, a discussion or a pull request for a security problem.

Helpful to include:

- what someone could do, and what they need for it (a file you open, a calendar address, another program on the computer);
- the steps to make it happen;
- the Keymotion version (Settings › About) and your Windows version.

## What happens next

Keymotion is maintained by one person, so this is best effort:

- you get a first reply within a week;
- a confirmed problem is fixed in a new version, with a note in the release about what was fixed;
- you are credited in that note if you'd like to be.

## What counts

In scope:

- the Keymotion app and its installer;
- how it handles files you open in it (pictures, profiles, backups);
- calendar addresses (they work like passwords; the app encrypts them for your Windows user);
- anything that makes Keymotion write to the keyboard without your action;
- `keymotion-cli` and the agent connection: anything that lets another Windows user, another computer, or a program while agent access is off light the keyboard, change Keymotion's settings, or change files through Keymotion; anything that lets an agent change what only you may change (below).

Out of scope:

- the keyboard's own firmware and AJAZZ's official software: please report those to AJAZZ;
- problems that need someone who already controls your Windows account.

## AI coding agents and keymotion-cli

- **Only you, on this computer.** Agents reach Keymotion through a named pipe made for your Windows user and this sign-in. Only your user may open it, it refuses clients from the network, and `keymotion-cli` checks that the pipe belongs to your user before it sends anything. There is no network listener and no token file.
- **Off until you turn it on.** Agent access is off when Keymotion is installed; you turn it on under **Tools › AI agents**. While it's off, agents can only read the status, the key names, the log and the settings. While it's on, any program that runs as you can ask for lights the way an agent does, as anything running as you could.
- **What agents can never change:** agent access, the permissions (the screen, the focus timer, profiles), the screen's hourly limit, **Agents may change these settings**, **Hide agent lights while playing**, and the connections to coding apps. The keyboard's firmware, key mapping and macros are out of their reach.
- **Limits that protect the keyboard and you.** Agents' lights live in memory only and end by themselves, after two hours at most; they flash at most three times a second and stay under the muted-microphone warning and meeting reminders. Each agent may send at most 20 requests a second and keep 8 lights, and can't clear another's.
- **Other programs' files only with your approval.** Keymotion changes Claude Code's and Codex's settings only after showing you what will change, and only its own entries. The rule it adds to Codex lets only `keymotion-cli`'s light and status commands out of Codex's sandbox, and not a copy of `keymotion-cli` that a repository brings along.

How it works in detail: [Agent command line](https://cebersaban44-rgb.github.io/keymotion/developers/agent-cli/#security).

## Supported versions

Fixes go into the latest release. Please update before reporting.
