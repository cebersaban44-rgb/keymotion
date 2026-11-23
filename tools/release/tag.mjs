#!/usr/bin/env node
// Tags a release on main and opens the back-merge pull request (docs site:
// developers/microsoft-store, "Making a release"). Run it on your computer,
// on main, after the release pull request (develop → main) is merged:
//
//   node tools/release/tag.mjs [--dry-run]
//
//   1. Checks: the branch is main, the working tree is clean, HEAD is
//      origin/main's tip, the version files agree (tools/check-version.mjs),
//      the version's CHANGELOGs, release notes and What's new are written
//      (tools/release/check-release.mjs), and the tag doesn't exist yet.
//   2. Makes the annotated tag vX.Y.Z ("Keymotion X.Y.Z") and pushes it,
//      which starts release.yml (and its Store job).
//   3. Opens the main → develop back-merge pull request with gh. CI runs on
//      it like on any other; merge it with a merge commit once it is green.
//
// A pre-release (1.3.0-beta.1; docs site: developers/contributing, "Beta
// (pre-release)") is tagged on its beta/* branch instead, which is never
// merged anywhere. There the checks are: the branch is beta/*, HEAD is its
// tip on GitHub, origin/main is in it (a beta has everything released), CI
// has passed on HEAD, and its CHANGELOG sections are written. release.yml
// then drafts a GitHub pre-release, with no Store package, and nothing is
// merged back.
//
// Both stay on a computer: only the repository's admins may make v* tags
// (the "Release tags" ruleset), and CI doesn't run on a pull request opened
// by a workflow's GITHUB_TOKEN. Nothing here merges anything or uses --admin.

import { execFileSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

import { appVersion, isPrerelease, releaseProblems } from "./check-release.mjs";

const root = fileURLToPath(new URL("../..", import.meta.url));

/** The branches a pre-release is tagged on. */
export const BETA_BRANCH = /^beta\/[^/]+$/;

/**
 * CI's state on a commit from its runs (`gh run list --json status,conclusion`):
 * "passed" when a run succeeded, "running" while one runs, "failed" when all
 * finished without success, "none" when there is no run.
 */
export function ciState(runs) {
  if (runs.some((r) => r.conclusion === "success")) return "passed";
  if (runs.some((r) => r.status !== "completed")) return "running";
  return runs.length ? "failed" : "none";
}

/** What keeps `state` from being tagged: an empty list when nothing does. */
export function tagProblems({ branch, clean, head, remoteHead, tagLocal, tagRemote, version, mainIsAncestor, ci }) {
  const problems = [];
  const short = (sha) => sha?.slice(0, 7);
  if (isPrerelease(version)) {
    if (!BETA_BRANCH.test(branch)) {
      problems.push(`the branch is ${branch}: a pre-release (${version}) is tagged on its own beta branch, cut from develop (beta/${version})`);
    }
    if (!clean) problems.push("the working tree has changes: commit or remove them first");
    if (!remoteHead) problems.push(`${branch} is not on GitHub: git push -u origin ${branch}`);
    else if (head !== remoteHead) problems.push(`HEAD (${short(head)}) is not origin/${branch} (${short(remoteHead)}): push it (git push), or pull what is missing`);
    if (!mainIsAncestor) {
      problems.push(`origin/main is not in ${branch}: a pre-release must have everything already released. Merge main back into develop and cut the beta again`);
    }
    if (ci === "running") problems.push(`CI is still running on HEAD (${short(head)}): tag once it is green (GitHub › Actions › CI)`);
    else if (ci === "failed") problems.push(`CI failed on HEAD (${short(head)}): fix it on develop and cut the beta again`);
    else if (ci !== "passed") problems.push(`CI has not run on HEAD (${short(head)}): push the branch, CI runs on beta/* branches`);
  } else {
    if (branch !== "main") problems.push(`the branch is ${branch}, not main (git switch main && git pull)`);
    if (!clean) problems.push("the working tree has changes: commit or remove them first");
    if (head !== remoteHead) problems.push(`HEAD (${short(head)}) is not origin/main (${short(remoteHead)}): git pull, or push what is missing through a pull request`);
  }
  if (tagLocal) problems.push(`tag v${version} already exists here`);
  if (tagRemote) problems.push(`tag v${version} already exists on GitHub`);
  return problems;
}

/** The back-merge pull request's title and body. */
export function backMerge(version) {
  return {
    title: `Merge main back into develop after ${version}`,
    body: `Back-merge after the ${version} release, with a merge commit, so \`develop\` is never behind \`main\`.`,
  };
}

/**
 * Makes the annotated tag and pushes it by its full ref. When the push
 * fails, the local tag is deleted again, so the next run doesn't stop on
 * "already exists here"; if that fails too, the command to do it is printed.
 */
export function tagAndPush({ tag, version, git, push, log = console.log }) {
  git("tag", "-a", tag, "-m", `Keymotion ${version}`);
  try {
    push(`refs/tags/${tag}`);
  } catch (e) {
    try {
      git("tag", "-d", tag);
      log(`The push failed, so the local tag ${tag} was deleted again. Fix the cause and run this again.`);
    } catch {
      log(`The push failed. Delete the local tag before running this again: git tag -d ${tag}`);
    }
    throw e;
  }
}

const git = (...args) => execFileSync("git", args, { cwd: root, encoding: "utf8" }).trim();
const gh = (...args) => execFileSync("gh", args, { cwd: root, encoding: "utf8" }).trim();

/** Whether git says yes (exit code 0) or no (1), as `merge-base --is-ancestor` does. */
function gitYes(...args) {
  try {
    execFileSync("git", args, { cwd: root, stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

/** The tip of `branch` on GitHub, fetched now, or null when it isn't there. */
function remoteTip(branch) {
  if (!gitYes("fetch", "--quiet", "origin", branch)) return null;
  return git("rev-parse", `origin/${branch}`);
}

function main() {
  const { values: opt } = parseArgs({ options: { "dry-run": { type: "boolean", default: false } } });
  const dry = opt["dry-run"];

  git("fetch", "--quiet", "--tags", "origin", "main");
  execFileSync(process.execPath, [join(root, "tools", "check-version.mjs")], { cwd: root, stdio: "inherit" });
  const version = appVersion(root);
  const prerelease = isPrerelease(version);
  const tag = `v${version}`;
  const branch = git("rev-parse", "--abbrev-ref", "HEAD");
  const head = git("rev-parse", "HEAD");
  const state = {
    version,
    branch,
    clean: git("status", "--porcelain") === "",
    head,
    remoteHead: prerelease ? remoteTip(branch) : git("rev-parse", "origin/main"),
    tagLocal: git("tag", "--list", tag) !== "",
    tagRemote: git("ls-remote", "--tags", "origin", `refs/tags/${tag}`) !== "",
  };
  if (prerelease) {
    state.mainIsAncestor = gitYes("merge-base", "--is-ancestor", "origin/main", "HEAD");
    state.ci = ciState(JSON.parse(gh("run", "list", "--workflow", "ci.yml", "--commit", head, "--json", "status,conclusion")));
  }
  const problems = [...tagProblems(state), ...releaseProblems(root, version).problems];
  if (problems.length) {
    console.log(`Can't tag ${tag}:`);
    for (const p of problems) console.log(`  - ${p}`);
    process.exit(1);
  }

  const push = (ref) => execFileSync("git", ["push", "origin", ref], { cwd: root, stdio: "inherit" });
  if (prerelease) {
    console.log(`Ready: ${tag} (a pre-release) on ${branch} (${head.slice(0, 7)}), which has everything on main.`);
    if (dry) {
      console.log(`Dry run. Would run:\n  git tag -a ${tag} -m "Keymotion ${version}"\n  git push origin refs/tags/${tag}`);
      console.log("No back-merge pull request: a beta branch is never merged.");
      return;
    }
    tagAndPush({ tag, version, git, push });
    console.log(`Pushed ${tag}: release.yml builds the installer and drafts a GitHub pre-release, with no Store package (GitHub › Actions).`);
    console.log(`Then publish it with Actions › Publish a release, run from the tag ${tag}: it stays a pre-release, never GitHub's latest release.`);
    return;
  }

  const pr = backMerge(version);
  console.log(`Ready: ${tag} on main (${state.head.slice(0, 7)}).`);
  if (dry) {
    console.log(`Dry run. Would run:\n  git tag -a ${tag} -m "Keymotion ${version}"\n  git push origin refs/tags/${tag}`);
    console.log(`  gh pr create --base develop --head main --title "${pr.title}"`);
    return;
  }

  tagAndPush({ tag, version, git, push });
  console.log(`Pushed ${tag}: release.yml builds the installer and the Store package (GitHub › Actions).`);

  const open = gh("pr", "list", "--base", "develop", "--head", "main", "--state", "open", "--json", "url", "--jq", ".[0].url");
  const url = open || gh("pr", "create", "--base", "develop", "--head", "main", "--title", pr.title, "--body", pr.body);
  console.log(`Back-merge pull request: ${url}`);
  console.log("Merge it with a merge commit (not squash) once CI is green.");
}

const isMain = process.argv[1] && pathToFileURL(realpathSync(process.argv[1])).href === import.meta.url;
if (isMain) {
  try {
    main();
  } catch (e) {
    console.log(e.message);
    process.exit(1);
  }
}
