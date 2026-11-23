// Tests for check-beta.mjs: its rules, and the script on a small made-up
// repository whose origin/* branches stand for GitHub's.
// Run: node --test "tools/release/*.test.mjs"

import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

import { betaProblems, CI_JOB, ciState, notPrepared } from "./check-beta.mjs";

const root = fileURLToPath(new URL("../..", import.meta.url));
const script = join(root, "tools", "release", "check-beta.mjs");
// Expected failures must not print GitHub Actions error annotations in CI.
const env = { ...process.env, GITHUB_ACTIONS: "" };

test("notPrepared: the version files and the CHANGELOGs are all a beta may change", () => {
  const prepared = [
    "Cargo.toml",
    "Cargo.lock",
    "crates/kb-core/Cargo.toml",
    "app/package.json",
    "app/src-tauri/tauri.conf.json",
    "app/src-tauri/Cargo.toml",
    "CHANGELOG.md",
    "CHANGELOG.tr.md",
    "CHANGELOG.zh-CN.md",
  ];
  assert.deepEqual(notPrepared(prepared), []);
  const others = [
    "crates/kb-core/src/lib.rs",
    "crates/kb-core/fuzz/Cargo.toml",
    "app/src/App.tsx",
    "app/pnpm-lock.yaml",
    "app/src-tauri/src/lib.rs",
    "site/src/content/docs/about/release-notes.mdx",
    "store/whats-new/1.3.0.en.txt",
    ".github/workflows/release.yml",
    "tools/release/tag.mjs",
    "docs/CHANGELOG.md",
    "CHANGELOG.txt",
  ];
  assert.deepEqual(notPrepared([...prepared, ...others]), others);
});

test('ciState: the "Build and test" job decides, not the whole run', () => {
  const run = (status, ...jobs) => ({ status, jobs: jobs.map(([name, conclusion, s = "completed"]) => ({ name, status: s, conclusion })) });
  const build = (conclusion, status) => [CI_JOB, conclusion, status];
  // A red cargo-deny or site job (not required for merging) doesn't hold a beta back.
  assert.equal(ciState([run("completed", build("success"), ["Supply chain (cargo-deny)", "failure"], ["Documentation site", "failure"])]), "passed");
  assert.equal(ciState([run("completed", build("failure"), ["Supply chain (cargo-deny)", "success"])]), "failed");
  // An older failed run and a green re-run: green.
  assert.equal(ciState([run("completed", build("failure")), run("completed", build("success"))]), "passed");
  // The job runs, or its run hasn't started it yet.
  assert.equal(ciState([run("in_progress", build("", "in_progress"))]), "running");
  assert.equal(ciState([run("queued")]), "running");
  // The job finished red while the other jobs still run: it failed.
  assert.equal(ciState([run("in_progress", build("failure"), ["Documentation site", "", "in_progress"])]), "failed");
  // Cancelled, or the run ended before the job began.
  assert.equal(ciState([run("completed", build("cancelled"))]), "failed");
  assert.equal(ciState([run("completed")]), "failed");
  assert.equal(ciState([]), "none");
});

test("betaProblems: main in it, prepare's changes only, CI green", () => {
  const ok = { branch: "origin/beta/1.3.0-beta.1", at: "abc1234", mainIsAncestor: true, unprepared: [], ci: "passed" };
  assert.deepEqual(betaProblems(ok), []);
  // CI left out: not checked.
  assert.deepEqual(betaProblems({ ...ok, ci: undefined }), []);
  assert.match(betaProblems({ ...ok, mainIsAncestor: false })[0], /^origin\/main is not in origin\/beta\/1\.3\.0-beta\.1: .*Merge main back into develop and cut the beta again$/);
  assert.match(
    betaProblems({ ...ok, unprepared: ["app/src/App.tsx"] })[0],
    /^origin\/beta\/1\.3\.0-beta\.1 changes more than prepare\.mjs does since develop \(app\/src\/App\.tsx\): .*Make the change on develop and cut the beta again$/,
  );
  const many = Array.from({ length: 7 }, (_, i) => `f${i}.rs`);
  assert.match(betaProblems({ ...ok, unprepared: many })[0], /\(f0\.rs, f1\.rs, f2\.rs, f3\.rs, f4\.rs and 2 more\)/);
  assert.match(betaProblems({ ...ok, ci: "running" })[0], /CI's "Build and test" job is still running on abc1234/);
  assert.match(betaProblems({ ...ok, ci: "failed" })[0], /CI's "Build and test" job did not pass on abc1234/);
  assert.match(betaProblems({ ...ok, ci: "none" })[0], /CI has not run on abc1234/);
  assert.equal(betaProblems({ ...ok, mainIsAncestor: false, unprepared: ["x"], ci: "failed" }).length, 3);
});

/**
 * A repository with main, develop and beta branches, their GitHub copies
 * as origin/*: 1.2.1 released on main; develop with work on top; a beta cut
 * from it with prepare's changes; a second beta with a code change too.
 */
function history(t) {
  const dir = mkdtempSync(join(tmpdir(), "keymotion-beta-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const git = (...args) =>
    execFileSync("git", ["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false", ...args], {
      cwd: dir,
      encoding: "utf8",
    }).trim();
  const commit = (message, files) => {
    for (const [file, text] of Object.entries(files)) {
      mkdirSync(dirname(join(dir, file)), { recursive: true });
      writeFileSync(join(dir, file), text);
    }
    git("add", "-A");
    git("commit", "-q", "-m", message);
    return git("rev-parse", "HEAD");
  };
  git("init", "-q", "-b", "main");
  const c = {};
  c.released = commit("Keymotion 1.2.1", { "Cargo.toml": 'version = "1.2.1"\n', "CHANGELOG.md": "## [1.2.1]\n", "app/src/app.ts": "1\n" });
  git("switch", "-q", "-c", "develop");
  c.work = commit("Work on develop", { "app/src/app.ts": "2\n", "CHANGELOG.md": "## [Unreleased]\n\n- Work.\n\n## [1.2.1]\n" });
  git("switch", "-q", "-c", "beta/1.3.0-beta.1");
  c.beta = commit("Prepare 1.3.0-beta.1", {
    "Cargo.toml": 'version = "1.3.0-beta.1"\n',
    "Cargo.lock": "lock\n",
    "crates/kb-core/Cargo.toml": "crate\n",
    "app/package.json": "{}\n",
    "app/src-tauri/tauri.conf.json": "{}\n",
    "CHANGELOG.md": "## [Unreleased]\n\n- Work.\n\n## [1.3.0-beta.1]\n\n- Work.\n\n## [1.2.1]\n",
    "CHANGELOG.tr.md": "tr\n",
  });
  git("switch", "-q", "-c", "beta/1.3.0-beta.2", c.beta);
  c.fixed = commit("A fix made on the beta", { "app/src/app.ts": "3\n", "app/src/new.ts": "4\n" });
  // develop goes on after the beta was cut; main gets a hotfix later.
  git("switch", "-q", "develop");
  c.later = commit("More work on develop", { "app/src/app.ts": "5\n" });
  git("switch", "-q", "main");
  c.hotfix = commit("Hotfix on main", { "app/src/hotfix.ts": "6\n" });
  const remote = (branch, sha) => git("update-ref", `refs/remotes/origin/${branch}`, sha);
  remote("main", c.released);
  remote("develop", c.later);
  remote("beta/1.3.0-beta.1", c.beta);
  remote("beta/1.3.0-beta.2", c.fixed);
  const check = (sha) => spawnSync(process.execPath, [script, sha, "--root", dir], { encoding: "utf8", env });
  return { c, remote, check };
}

test("CLI: a beta commit on its pushed branch, with main in it and prepare's changes only", (t) => {
  const { c, remote, check } = history(t);
  const short = (sha) => sha.slice(0, 7);

  // develop has moved on since the cut: the beta is still compared with the commit it came from.
  const ok = check(c.beta);
  assert.equal(ok.status, 0, ok.stdout + ok.stderr);
  assert.match(ok.stdout, new RegExp(`^Beta ${short(c.beta)} is on origin/beta/1\\.3\\.0-beta\\.1, origin/beta/1\\.3\\.0-beta\\.2, has everything on main and changes only the version and the CHANGELOGs since develop\\.\\n$`));

  // A code change on the beta branch.
  const fixed = check(c.fixed);
  assert.equal(fixed.status, 1);
  assert.match(fixed.stdout, /changes more than prepare\.mjs does since develop \(app\/src\/app\.ts, app\/src\/new\.ts\)/);

  // A commit on no beta branch (develop's tip).
  const later = check(c.later);
  assert.equal(later.status, 1);
  assert.match(later.stdout, new RegExp(`Beta ${short(c.later)}: ${short(c.later)} is on no beta/\\* branch on GitHub`));

  // main moves on (a hotfix not merged back into develop): the beta lacks it.
  remote("main", c.hotfix);
  const behind = check(c.beta);
  assert.equal(behind.status, 1);
  assert.match(behind.stdout, /origin\/main is not in origin\/beta\/1\.3\.0-beta\.1, origin\/beta\/1\.3\.0-beta\.2: a pre-release must have everything already released/);
  assert.doesNotMatch(behind.stdout, /::error::/);
});
