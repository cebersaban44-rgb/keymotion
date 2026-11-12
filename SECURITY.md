# Security

Keymotion runs on your computer, talks to your keyboard over USB and Bluetooth, and reads the calendar addresses you give it. If you find a way to misuse any of that, please tell us privately first, so it can be fixed before it's public.

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
- anything that makes Keymotion write to the keyboard without your action.

Out of scope:

- the keyboard's own firmware and AJAZZ's official software: please report those to AJAZZ;
- problems that need someone who already controls your Windows account.

## Supported versions

Fixes go into the latest release. Please update before reporting.
