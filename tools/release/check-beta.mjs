#!/usr/bin/env node
// Is a commit a beta that may be tagged and built? (docs site:
// developers/contributing, "Beta (pre-release)"). tools/release/tag.mjs
// checks it before it tags a pre-release, and release.yml again on the
// pre-release tag's commit, so a beta tag pushed by hand meets the same
// rules:
//   - the commit is on a beta/* branch on GitHub (release.yml; tag.mjs
//     checks its own branch);
//   - it has everything already released: origin/main is in it;
//   - since develop (its merge base with origin/develop) it changes only
//     what tools/release/prepare.mjs changes for a pre-release: the version
//     files and the CHANGELOGs. A fix goes to develop, and the beta is cut
//     again;
//   - CI's "Build and test" job passed on it (--ci, read with gh). It is the
//     job branch protection requires; the others (cargo-deny, the site) are
//     not required for merging either, so they don't hold a beta back.
//
// Usage: node tools/release/check-beta.mjs <commit> [--ci]
//   with origin/main, origin/develop and origin/beta/* fetched (release.yml
//   checks out every branch).

import { execFileSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

import { CHANGELOG_FILE } from "./check-release.mjs";

const repoRoot = fileURLToPath(new URL("../..", import.meta.url));

/** The files prepare.mjs changes for a pre-release: the version's and the CHANGELOGs. */
export const PREPARED_FILES = [
  /^Cargo\.(?:toml|lock)$/,
  /^crates\/[^/]+\/Cargo\.toml$/,
  /^app\/package\.json$/,
  /^app\/src-tauri\/(?:tauri\.conf\.json|Cargo\.toml)$/,
  CHANGELOG_FILE,
];

/** The files in `files` that prepare.mjs does not change. */
export const notPrepared = (files) => files.filter((f) => !PREPARED_FILES.some((re) => re.test(f)));

/** The CI job a beta needs: the one branch protection requires before a merge. */
export const CI_JOB = "Build and test";

/**
 * CI's state on a commit, from its ci.yml runs and their jobs
 * (`[{ status, jobs: [{ name, status, conclusion }] }]`): "passed" when a
 * run's "Build and test" job succeeded, "running" while one may still,
 * "failed" when every run finished without it, "none" when there is no run.
 */
export function ciState(runs) {
  const job = (run) => run.jobs?.find((j) => j.name === CI_JOB);
  if (runs.some((r) => job(r)?.conclusion === "success")) return "passed";
  if (runs.some((r) => (job(r) ? job(r).status !== "completed" : r.status !== "completed"))) return "running";
  return runs.length ? "failed" : "none";
}

/**
 * What keeps a beta commit from being tagged or built, besides its branch:
 * main not in it, changes prepare.mjs doesn't make, CI not green (`ci` left
 * out: not checked). `branch` and `at` name the beta and the commit in the
 * messages.
 */
export function betaProblems({ branch, at, mainIsAncestor, unprepared = [], ci }) {
  const problems = [];
  if (!mainIsAncestor) {
    problems.push(`origin/main is not in ${branch}: a pre-release must have everything already released. Merge main back into develop and cut the beta again`);
  }
  if (unprepared.length) {
    const shown = unprepared.slice(0, 5).join(", ") + (unprepared.length > 5 ? ` and ${unprepared.length - 5} more` : "");
    problems.push(
      `${branch} changes more than prepare.mjs does since develop (${shown}): a beta is develop with only its version and CHANGELOG sections. ` +
        "Make the change on develop and cut the beta again",
    );
  }
  if (ci === "running") problems.push(`CI's "${CI_JOB}" job is still running on ${at}: wait until it is green (GitHub › Actions › CI)`);
  else if (ci === "failed") problems.push(`CI's "${CI_JOB}" job did not pass on ${at}: fix it on develop and cut the beta again`);
  else if (ci === "none") problems.push(`CI has not run on ${at}: push the branch, CI runs on beta/* branches`);
  return problems;
}

/** The files commit `sha` changes since develop (its merge base with origin/develop). */
export function changedSinceDevelop(git, sha) {
  const base = git("merge-base", "origin/develop", sha);
  return git("diff", "--name-only", "--no-renames", base, sha).split("\n").filter(Boolean);
}

/** ci.yml's runs on commit `sha`, each with its jobs. */
export function ciRuns(gh, sha) {
  const runs = JSON.parse(gh("run", "list", "--workflow", "ci.yml", "--commit", sha, "--json", "databaseId,status,conclusion", "--limit", "20"));
  return runs.map((run) => ({ ...run, jobs: JSON.parse(gh("run", "view", String(run.databaseId), "--json", "jobs")).jobs }));
}

function main() {
  const { values: opt, positionals } = parseArgs({
    allowPositionals: true,
    options: { ci: { type: "boolean", default: false }, root: { type: "string" } },
  });
  if (positionals.length !== 1) throw new Error("usage: check-beta.mjs <commit> [--ci]");
  const cwd = opt.root ?? repoRoot;
  const git = (...args) => execFileSync("git", args, { cwd, encoding: "utf8" }).trim();
  const gh = (...args) => execFileSync("gh", args, { cwd, encoding: "utf8" }).trim();
  const yes = (...args) => {
    try {
      execFileSync("git", args, { cwd, stdio: "ignore" });
      return true;
    } catch {
      return false;
    }
  };

  const sha = git("rev-parse", `${positionals[0]}^{commit}`);
  const short = sha.slice(0, 7);
  const branches = git("for-each-ref", "--contains", sha, "--format=%(refname:short)", "refs/remotes/origin/beta/").split("\n").filter(Boolean);
  const problems = [];
  if (!branches.length) problems.push(`${short} is on no beta/* branch on GitHub: tag a pre-release on its pushed beta branch with tools/release/tag.mjs`);
  const branch = branches.length ? branches.join(", ") : short;
  problems.push(
    ...betaProblems({
      branch,
      at: short,
      mainIsAncestor: yes("merge-base", "--is-ancestor", "origin/main", sha),
      unprepared: notPrepared(changedSinceDevelop(git, sha)),
      ...(opt.ci ? { ci: ciState(ciRuns(gh, sha)) } : {}),
    }),
  );

  const prefix = process.env.GITHUB_ACTIONS ? "::error::" : "";
  if (problems.length) {
    for (const p of problems) console.log(`${prefix}Beta ${short}: ${p}`);
    process.exit(1);
  }
  console.log(
    `Beta ${short} is on ${branch}, has everything on main and changes only the version and the CHANGELOGs since develop` +
      (opt.ci ? `; CI's "${CI_JOB}" job passed on it.` : "."),
  );
}

const isMain = process.argv[1] && pathToFileURL(realpathSync(process.argv[1])).href === import.meta.url;
if (isMain) {
  try {
    main();
  } catch (e) {
    console.log(`${process.env.GITHUB_ACTIONS ? "::error::" : ""}${e.message}`);
    process.exit(1);
  }
}
