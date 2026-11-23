// Tests for tag.mjs's checks (the script itself runs git and gh on a computer).
// Run: node --test "tools/release/*.test.mjs"

import assert from "node:assert/strict";
import { test } from "node:test";

import { BETA_BRANCH, backMerge, ciState, tagAndPush, tagProblems } from "./tag.mjs";

const ready = {
  version: "1.2.0",
  branch: "main",
  clean: true,
  head: "a".repeat(40),
  remoteHead: "a".repeat(40),
  tagLocal: false,
  tagRemote: false,
};

test("tagProblems: main's tip, clean, and a tag that doesn't exist yet", () => {
  assert.deepEqual(tagProblems(ready), []);
  assert.match(tagProblems({ ...ready, branch: "develop" })[0], /the branch is develop, not main/);
  assert.match(tagProblems({ ...ready, clean: false })[0], /working tree has changes/);
  assert.match(tagProblems({ ...ready, remoteHead: "b".repeat(40) })[0], /HEAD \(aaaaaaa\) is not origin\/main \(bbbbbbb\)/);
  assert.match(tagProblems({ ...ready, tagLocal: true })[0], /tag v1\.2\.0 already exists here/);
  assert.match(tagProblems({ ...ready, tagRemote: true })[0], /tag v1\.2\.0 already exists on GitHub/);
  assert.equal(tagProblems({ ...ready, branch: "x", clean: false, tagRemote: true }).length, 3);
  // A release is never tagged on a beta branch, whatever the beta checks say.
  assert.match(tagProblems({ ...ready, branch: "beta/1.2.0-beta.1", mainIsAncestor: true, ci: "passed" })[0], /the branch is beta\/1\.2\.0-beta\.1, not main/);
});

const beta = {
  version: "1.3.0-beta.1",
  branch: "beta/1.3.0-beta.1",
  clean: true,
  head: "c".repeat(40),
  remoteHead: "c".repeat(40),
  mainIsAncestor: true,
  ci: "passed",
  tagLocal: false,
  tagRemote: false,
};

test("tagProblems: a pre-release on its pushed beta branch, with main in it and CI green", () => {
  assert.deepEqual(tagProblems(beta), []);
  // Any beta/* branch; never main, develop or a nested name.
  assert.deepEqual(tagProblems({ ...beta, branch: "beta/1.3.0" }), []);
  for (const branch of ["main", "develop", "release/1.3.0", "beta", "beta/a/b", "HEAD"]) {
    assert.match(tagProblems({ ...beta, branch })[0], /a pre-release \(1\.3\.0-beta\.1\) is tagged on its own beta branch, cut from develop \(beta\/1\.3\.0-beta\.1\)/, branch);
  }
  assert.ok(BETA_BRANCH.test("beta/1.3.0-beta.1"));
  assert.match(tagProblems({ ...beta, clean: false })[0], /working tree has changes/);
  assert.match(tagProblems({ ...beta, remoteHead: null })[0], /beta\/1\.3\.0-beta\.1 is not on GitHub: git push -u origin beta\/1\.3\.0-beta\.1/);
  assert.match(tagProblems({ ...beta, remoteHead: "d".repeat(40) })[0], /HEAD \(ccccccc\) is not origin\/beta\/1\.3\.0-beta\.1 \(ddddddd\): push it/);
  assert.match(tagProblems({ ...beta, mainIsAncestor: false })[0], /origin\/main is not in beta\/1\.3\.0-beta\.1: a pre-release must have everything already released/);
  assert.match(tagProblems({ ...beta, ci: "running" })[0], /CI is still running on HEAD \(ccccccc\)/);
  assert.match(tagProblems({ ...beta, ci: "failed" })[0], /CI failed on HEAD \(ccccccc\)/);
  assert.match(tagProblems({ ...beta, ci: "none" })[0], /CI has not run on HEAD \(ccccccc\)/);
  assert.match(tagProblems({ ...beta, ci: undefined })[0], /CI has not run on HEAD/);
  assert.match(tagProblems({ ...beta, tagRemote: true })[0], /tag v1\.3\.0-beta\.1 already exists on GitHub/);
  assert.equal(tagProblems({ ...beta, branch: "develop", mainIsAncestor: false, ci: "failed", tagLocal: true }).length, 4);
});

test("ciState: a success passes; otherwise running, failed or none", () => {
  const done = (conclusion) => ({ status: "completed", conclusion });
  assert.equal(ciState([done("success")]), "passed");
  // A re-run that passed counts, whatever an older run says.
  assert.equal(ciState([done("failure"), done("success")]), "passed");
  assert.equal(ciState([{ status: "in_progress", conclusion: "" }]), "running");
  assert.equal(ciState([done("failure"), { status: "queued", conclusion: "" }]), "running");
  assert.equal(ciState([done("failure")]), "failed");
  assert.equal(ciState([done("cancelled")]), "failed");
  assert.equal(ciState([]), "none");
});

test("tagAndPush: the full ref is pushed; a refused push takes the local tag away again", () => {
  const run = ({ pushFails = false, deleteFails = false } = {}) => {
    const calls = [];
    const logs = [];
    const git = (...args) => {
      calls.push(["git", ...args]);
      if (deleteFails && args[1] === "-d") throw new Error("cannot delete");
    };
    const push = (ref) => {
      calls.push(["push", ref]);
      if (pushFails) throw new Error("push refused");
    };
    let error = null;
    try {
      tagAndPush({ tag: "v1.2.0", version: "1.2.0", git, push, log: (l) => logs.push(l) });
    } catch (e) {
      error = e;
    }
    return { calls, logs, error };
  };

  const ok = run();
  assert.equal(ok.error, null);
  assert.deepEqual(ok.calls, [["git", "tag", "-a", "v1.2.0", "-m", "Keymotion 1.2.0"], ["push", "refs/tags/v1.2.0"]]);

  const refused = run({ pushFails: true });
  assert.match(refused.error.message, /push refused/);
  assert.deepEqual(refused.calls.at(-1), ["git", "tag", "-d", "v1.2.0"]);
  assert.match(refused.logs[0], /local tag v1\.2\.0 was deleted again/);

  const stuck = run({ pushFails: true, deleteFails: true });
  assert.match(stuck.error.message, /push refused/);
  assert.match(stuck.logs[0], /git tag -d v1\.2\.0$/);
});

test("backMerge: the pull request says why it is a merge commit", () => {
  assert.deepEqual(backMerge("1.2.0"), {
    title: "Merge main back into develop after 1.2.0",
    body: "Back-merge after the 1.2.0 release, with a merge commit, so `develop` is never behind `main`.",
  });
});
