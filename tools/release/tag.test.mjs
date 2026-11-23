// Tests for tag.mjs's checks (the script itself runs git and gh on a computer).
// Run: node --test "tools/release/*.test.mjs"

import assert from "node:assert/strict";
import { test } from "node:test";

import { backMerge, tagProblems } from "./tag.mjs";

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

test("backMerge: the pull request says why it is a merge commit", () => {
  assert.deepEqual(backMerge("1.2.0"), {
    title: "Merge main back into develop after 1.2.0",
    body: "Back-merge after the 1.2.0 release, with a merge commit, so `develop` is never behind `main`.",
  });
});
