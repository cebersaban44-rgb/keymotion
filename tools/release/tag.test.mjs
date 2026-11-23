// Tests for tag.mjs's checks (the script itself runs git and gh on a computer).
// Run: node --test "tools/release/*.test.mjs"

import assert from "node:assert/strict";
import { test } from "node:test";

import { backMerge, tagAndPush, tagProblems } from "./tag.mjs";

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
