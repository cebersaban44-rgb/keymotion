// Tests for submit.mjs and whats-new.mjs. The Store API is a fake fetch:
// nothing here goes to Microsoft.
// Run: node --test "tools/store/*.test.mjs"

import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";
import { crc32 } from "node:zlib";

import { STORE_URL } from "../release/latest-json.mjs";
import {
  STORE_APP_ID,
  findMsix,
  matchListings,
  prepareSubmission,
  storeClient,
  storeSecrets,
  storeStatus,
  submit,
  zipOne,
} from "./submit.mjs";
import { STORE_LANGUAGES, WHATS_NEW_MAX, readWhatsNew, whatsNewProblem, whatsNewSkeleton } from "./whats-new.mjs";

const root = fileURLToPath(new URL("../..", import.meta.url));
const script = join(root, "tools", "store", "submit.mjs");

const API = `https://manage.devcenter.microsoft.com/v1.0/my/applications/${STORE_APP_ID}`;
const UPLOAD_URL = "https://ingest.blob.example/ingestion/abc?sv=2020&sig=SAS-SECRET";
const SECRETS = { tenantId: "tenant-1", clientId: "client-1", clientSecret: "CLIENT-SECRET" };
const NOTES = { en: "Music mode.", tr: "Müzik modu." };
const MSIX = { name: "Keymotion_1.2.0.0_x64.msix", bytes: Buffer.from("not really a package") };

const listing = (title) => ({ baseListing: { title, description: "d", releaseNotes: "Old notes.", images: [] }, platformOverrides: {} });

/** A submission as the Store copies it from the last published one. */
const copy = (id, extra = {}) => ({
  id,
  status: "PendingCommit",
  targetPublishMode: "Immediate",
  fileUploadUrl: UPLOAD_URL,
  listings: { "en-us": listing("Keymotion"), "tr-tr": listing("Keymotion") },
  applicationPackages: [{ fileName: "Keymotion_1.1.0.0_x64.msix", fileStatus: "Uploaded", id: "p1", version: "1.1.0.0" }],
  ...extra,
});

/**
 * A fake Store: `routes` answers "METHOD path" (the path after the app) with
 * a body or [status, body], or with `{ responses: [...] }` in turn (the last
 * one again after that); every request is kept in `calls`.
 */
function fakeStore(routes = {}) {
  const calls = [];
  const turns = {};
  async function fetch(url, init = {}) {
    const method = init.method ?? "GET";
    calls.push({ method, url: String(url), headers: init.headers ?? {}, body: init.body });
    if (String(url).startsWith("https://login.microsoftonline.com/")) {
      return Response.json({ token_type: "Bearer", expires_in: "3599", access_token: "TOKEN" });
    }
    if (String(url) === UPLOAD_URL) return new Response(null, { status: 201 });
    const key = `${method} ${String(url).slice(API.length) || "/"}`;
    let answer = routes[key];
    if (answer === undefined) return Response.json({ code: "NotFound", message: `no route ${key}` }, { status: 404 });
    if (Array.isArray(answer.responses)) {
      turns[key] = (turns[key] ?? -1) + 1;
      answer = answer.responses[Math.min(turns[key], answer.responses.length - 1)];
    }
    const [status, body] = Array.isArray(answer) && typeof answer[0] === "number" ? answer : [200, answer];
    return Response.json(body, { status });
  }
  const api = () => calls.filter((c) => c.url.startsWith(API)).map((c) => `${c.method} ${c.url.slice(API.length) || "/"}`);
  return { fetch, calls, api };
}

const noSleep = async () => {};
const client = (store, extra = {}) => storeClient({ ...SECRETS, fetch: store.fetch, sleep: noSleep, ...extra });

/** The one file in a ZIP that zipOne made: its name and bytes, with every field checked. */
function unzip(zip) {
  assert.equal(zip.readUInt32LE(0), 0x04034b50);
  const nameLength = zip.readUInt16LE(26);
  const size = zip.readUInt32LE(22);
  const name = zip.subarray(30, 30 + nameLength).toString("utf8");
  const data = zip.subarray(30 + nameLength, 30 + nameLength + size);
  assert.equal(zip.readUInt16LE(8), 0, "stored");
  assert.equal(zip.readUInt32LE(14), crc32(data));
  const end = zip.subarray(zip.length - 22);
  assert.equal(end.readUInt32LE(0), 0x06054b50);
  assert.equal(end.readUInt16LE(10), 1);
  const central = zip.subarray(end.readUInt32LE(16));
  assert.equal(central.readUInt32LE(0), 0x02014b50);
  assert.equal(central.readUInt32LE(16), crc32(data));
  assert.equal(central.readUInt32LE(42), 0);
  assert.equal(central.subarray(46, 46 + nameLength).toString("utf8"), name);
  assert.equal(end.readUInt32LE(12), 46 + nameLength);
  return { name, data };
}

test("the Store ID is the one the release page links", () => {
  assert.ok(STORE_URL.includes(`/detail/${STORE_APP_ID}?`));
});

test("storeSecrets: none is a skip, some is a mistake, all three sign in", () => {
  assert.equal(storeSecrets({}), null);
  assert.equal(storeSecrets({ STORE_TENANT_ID: " ", STORE_CLIENT_ID: "" }), null);
  assert.throws(() => storeSecrets({ STORE_TENANT_ID: "t", STORE_CLIENT_ID: "c" }), /STORE_CLIENT_SECRET not set, while STORE_TENANT_ID, STORE_CLIENT_ID is/);
  assert.deepEqual(storeSecrets({ STORE_TENANT_ID: "t", STORE_CLIENT_ID: "c", STORE_CLIENT_SECRET: "s\n" }), {
    tenantId: "t",
    clientId: "c",
    clientSecret: "s",
  });
});

test("the client signs in once with client credentials and sends the token", async () => {
  const store = fakeStore({ "GET /": { id: STORE_APP_ID } });
  const c = client(store);
  await c.getApp();
  await c.getApp();
  const [token, ...rest] = store.calls;
  assert.equal(token.method, "POST");
  assert.equal(token.url, "https://login.microsoftonline.com/tenant-1/oauth2/token");
  assert.equal(token.headers["Content-Type"], "application/x-www-form-urlencoded");
  assert.deepEqual(Object.fromEntries(new URLSearchParams(token.body)), {
    grant_type: "client_credentials",
    client_id: "client-1",
    client_secret: "CLIENT-SECRET",
    resource: "https://manage.devcenter.microsoft.com",
  });
  assert.equal(rest.length, 2);
  for (const call of rest) assert.equal(call.headers.Authorization, "Bearer TOKEN");
});

test("a failed sign-in names the secrets, not their values", async () => {
  const fetch = async () => Response.json({ error: "invalid_client", error_description: "AADSTS7000215: Invalid client secret.\r\nTrace ID: x" }, { status: 401 });
  const err = await client({ fetch }).getApp().catch((e) => e);
  assert.match(err.message, /Microsoft Entra sign-in failed \(HTTP 401\): AADSTS7000215: Invalid client secret\. Check STORE_TENANT_ID/);
  assert.doesNotMatch(err.message, /CLIENT-SECRET|Trace ID/);
});

test("GET is tried again on a server error; the error says what failed", async () => {
  const store = fakeStore({ "GET /": { responses: [[503, { message: "busy" }], [200, { id: STORE_APP_ID }]] } });
  assert.deepEqual(await client(store).getApp(), { id: STORE_APP_ID });
  assert.deepEqual(store.api(), ["GET /", "GET /"]);
  const failing = fakeStore({ "POST /submissions": [409, { code: "InvalidState", message: "A submission is pending." }] });
  await assert.rejects(client(failing).createSubmission(), /Store API POST \/submissions: HTTP 409 \(InvalidState: A submission is pending\.\)/);
  // POST is never repeated.
  assert.deepEqual(failing.api(), ["POST /submissions"]);
});

test("matchListings: each listing gets its language's notes", () => {
  assert.deepEqual(matchListings(["en-us", "tr-tr"], ["en", "tr"]), { pairs: [["en-us", "en"], ["tr-tr", "tr"]], missing: [], unused: [] });
  assert.deepEqual(matchListings(["EN-US", "de-de"], ["en", "tr"]), { pairs: [["EN-US", "en"]], missing: ["de-de"], unused: ["tr"] });
  assert.deepEqual(matchListings(["en"], ["en"]).pairs, [["en", "en"]]);
});

test("prepareSubmission: new package, old one PendingDelete, What's new, Manual every time", () => {
  const before = copy("200");
  const s = prepareSubmission(before, { packageFile: MSIX.name, notes: NOTES });
  assert.deepEqual(s.applicationPackages, [
    { fileName: "Keymotion_1.1.0.0_x64.msix", fileStatus: "PendingDelete", id: "p1", version: "1.1.0.0" },
    { fileName: MSIX.name, fileStatus: "PendingUpload", minimumDirectXVersion: "None", minimumSystemRam: "None" },
  ]);
  assert.equal(s.listings["en-us"].baseListing.releaseNotes, "Music mode.");
  assert.equal(s.listings["tr-tr"].baseListing.releaseNotes, "Müzik modu.");
  assert.equal(s.listings["tr-tr"].baseListing.title, "Keymotion");
  assert.equal(s.targetPublishMode, "Manual");
  // The rest is sent back as it came; the input is not changed.
  assert.equal(s.fileUploadUrl, UPLOAD_URL);
  assert.deepEqual(before, copy("200"));
  // Every listing needs notes, and all notes a listing.
  assert.throws(
    () => prepareSubmission(copy("200", { listings: { ...before.listings, "de-de": listing("K") } }), { packageFile: MSIX.name, notes: NOTES }),
    /listing has de-de, which has no What's new file/,
  );
  assert.throws(() => prepareSubmission(copy("200"), { packageFile: MSIX.name, notes: { ...NOTES, ru: "x" } }), /What's new in ru has no Store listing/);
});

test("submit: makes, fills, uploads, commits and waits while the commit runs", async () => {
  const store = fakeStore({
    "GET /": { id: STORE_APP_ID, lastPublishedApplicationSubmission: { id: "100" } },
    "GET /submissions/100": copy("100", { status: "Published" }),
    "POST /submissions": copy("200"),
    "PUT /submissions/200": copy("200", { targetPublishMode: "Manual" }),
    "POST /submissions/200/commit": { status: "CommitStarted" },
    "GET /submissions/200/status": { responses: [{ status: "CommitStarted" }, { status: "PreProcessing", statusDetails: { errors: [], warnings: [{ code: "W1", details: "a warning" }] } }] },
  });
  const logs = [];
  const sleeps = [];
  const result = await submit({ client: client(store), msix: MSIX, notes: NOTES, log: (l) => logs.push(l), sleep: async (ms) => sleeps.push(ms), pollMs: 1000 });
  assert.deepEqual(result, { id: "200", status: "PreProcessing" });
  assert.deepEqual(store.api(), [
    "GET /",
    "GET /submissions/100",
    "POST /submissions",
    "PUT /submissions/200",
    "POST /submissions/200/commit",
    "GET /submissions/200/status",
    "GET /submissions/200/status",
  ]);
  assert.deepEqual(sleeps, [1000, 1000]);

  const put = store.calls.find((c) => c.method === "PUT" && c.url.startsWith(API));
  assert.equal(put.headers["Content-Type"], "application/json");
  const body = JSON.parse(put.body);
  assert.equal(body.targetPublishMode, "Manual");
  assert.equal(body.listings["en-us"].baseListing.releaseNotes, "Music mode.");
  assert.deepEqual(body.applicationPackages.map((p) => [p.fileName, p.fileStatus]), [
    ["Keymotion_1.1.0.0_x64.msix", "PendingDelete"],
    [MSIX.name, "PendingUpload"],
  ]);

  // The ZIP goes to the submission's blob, before the commit.
  const upload = store.calls.find((c) => c.url === UPLOAD_URL);
  assert.equal(upload.method, "PUT");
  assert.equal(upload.headers["x-ms-blob-type"], "BlockBlob");
  assert.ok(store.calls.indexOf(upload) < store.calls.findIndex((c) => c.url.endsWith("/commit")));
  const file = unzip(Buffer.from(upload.body));
  assert.equal(file.name, MSIX.name);
  assert.deepEqual(file.data, MSIX.bytes);

  // Nothing secret in what it says, and nothing is ever deleted.
  const said = logs.join("\n");
  assert.match(said, /Made submission 200 \(a copy of 100\)\./);
  assert.match(said, /Warning W1: a warning/);
  assert.match(said, /Submission 200: PreProcessing\./);
  assert.doesNotMatch(said, /SAS-SECRET|TOKEN|CLIENT-SECRET/);
  assert.equal(store.calls.filter((c) => c.method === "DELETE").length, 0);
});

test("submit: a pending submission stops it before anything is made or changed", async () => {
  const store = fakeStore({
    "GET /": { id: STORE_APP_ID, pendingApplicationSubmission: { id: "150" }, lastPublishedApplicationSubmission: { id: "100" } },
  });
  await assert.rejects(submit({ client: client(store), msix: MSIX, notes: NOTES, log: () => {} }), /submission 150 is pending in Partner Center, so nothing was changed/);
  assert.deepEqual(store.api(), ["GET /"]);
});

test("submit: a listing without What's new stops it before a submission is made", async () => {
  const store = fakeStore({
    "GET /": { id: STORE_APP_ID, lastPublishedApplicationSubmission: { id: "100" } },
    "GET /submissions/100": copy("100", { listings: { "en-us": listing("K"), "tr-tr": listing("K"), "de-de": listing("K") } }),
  });
  await assert.rejects(submit({ client: client(store), msix: MSIX, notes: NOTES, log: () => {} }), /de-de, which has no What's new file/);
  assert.deepEqual(store.api(), ["GET /", "GET /submissions/100"]);
});

test("submit: a failure after the submission is made leaves it, never deleted", async () => {
  const store = fakeStore({
    "GET /": { id: STORE_APP_ID, lastPublishedApplicationSubmission: { id: "100" } },
    "GET /submissions/100": copy("100"),
    "POST /submissions": copy("200"),
    "PUT /submissions/200": copy("200", { targetPublishMode: "Manual" }),
    "POST /submissions/200/commit": { status: "CommitStarted" },
    "GET /submissions/200/status": { status: "CommitFailed", statusDetails: { errors: [{ code: "InvalidPackage", details: "The package is not valid." }] } },
  });
  const logs = [];
  const err = await submit({ client: client(store), msix: MSIX, notes: NOTES, log: (l) => logs.push(l), sleep: noSleep }).catch((e) => e);
  assert.match(err.message, /the submission is CommitFailed\nSubmission 200 was made and is left as it is \(this script never deletes one\)/);
  assert.match(logs.join("\n"), /Error InvalidPackage: The package is not valid\./);
  assert.equal(store.calls.filter((c) => c.method === "DELETE").length, 0);

  // A Store that keeps publishing by itself is refused before the upload.
  const immediate = fakeStore({
    "GET /": { id: STORE_APP_ID, lastPublishedApplicationSubmission: { id: "100" } },
    "GET /submissions/100": copy("100"),
    "POST /submissions": copy("200"),
    "PUT /submissions/200": copy("200", { targetPublishMode: "Immediate" }),
  });
  await assert.rejects(submit({ client: client(immediate), msix: MSIX, notes: NOTES, log: () => {} }), /targetPublishMode Immediate, not Manual/);
  assert.equal(immediate.calls.some((c) => c.url === UPLOAD_URL), false);
});

test("submit: a submission made without an upload address or id is reported as left behind", async () => {
  const made = (created) =>
    fakeStore({
      "GET /": { id: STORE_APP_ID, lastPublishedApplicationSubmission: { id: "100" } },
      "GET /submissions/100": copy("100"),
      "POST /submissions": created,
    });
  const noUrl = made(copy("200", { fileUploadUrl: undefined }));
  await assert.rejects(
    submit({ client: client(noUrl), msix: MSIX, notes: NOTES, log: () => {} }),
    /without an id or an upload address\nSubmission 200 was made and is left as it is/,
  );
  assert.deepEqual(noUrl.api(), ["GET /", "GET /submissions/100", "POST /submissions"]);
  await assert.rejects(
    submit({ client: client(made({ status: "PendingCommit" })), msix: MSIX, notes: NOTES, log: () => {} }),
    /\nA new submission \(the Store gave no id\) was made and is left as it is/,
  );
});

test("submit: a commit that runs too long stops waiting", async () => {
  let t = 0;
  const store = fakeStore({
    "GET /": { id: STORE_APP_ID, lastPublishedApplicationSubmission: { id: "100" } },
    "GET /submissions/100": copy("100"),
    "POST /submissions": copy("200"),
    "PUT /submissions/200": copy("200", { targetPublishMode: "Manual" }),
    "POST /submissions/200/commit": { status: "CommitStarted" },
    "GET /submissions/200/status": { status: "CommitStarted" },
  });
  const sleep = async (ms) => {
    t += ms;
  };
  await assert.rejects(
    submit({ client: client(store), msix: MSIX, notes: NOTES, log: () => {}, sleep, now: () => t, pollMs: 60_000, timeoutMs: 5 * 60_000 }),
    /still running after 5 minutes/,
  );
});

test("storeStatus: only reads, and finds the submission that carries the version", async () => {
  const pkg = (version, fileStatus = "Uploaded") => [{ fileName: `Keymotion_${version}_x64.msix`, fileStatus, version }];
  const store = fakeStore({
    "GET /": { id: STORE_APP_ID, pendingApplicationSubmission: { id: "200" }, lastPublishedApplicationSubmission: { id: "100" } },
    "GET /submissions/200": copy("200", { status: "Release", applicationPackages: [...pkg("1.1.0.0", "PendingDelete"), ...pkg("1.2.0.0")] }),
    "GET /submissions/100": copy("100", { status: "Published", applicationPackages: pkg("1.1.0.0") }),
  });
  const c = client(store);
  assert.deepEqual(await storeStatus({ client: c, version: "1.2.0" }), { id: "200", status: "Release", details: undefined, pending: true });
  assert.equal((await storeStatus({ client: c, version: "1.1.0" })).status, "Published");
  assert.equal((await storeStatus({ client: c, version: "1.3.0" })).status, "NotFound");
  assert.ok(store.calls.every((c) => c.method === "GET" || c.url.startsWith("https://login.microsoftonline.com/")));
});

test("zipOne: one stored file, readable back", () => {
  const data = Buffer.from("x".repeat(1000));
  const zip = zipOne("Keymotion_1.2.0.0_x64.msix", data, new Date(2026, 10, 30, 12, 34, 56));
  assert.deepEqual(unzip(zip).data, data);
  // DOS time and date: 12:34:56, 2026-11-30.
  assert.equal(zip.readUInt16LE(10), (12 << 11) | (34 << 5) | 28);
  assert.equal(zip.readUInt16LE(12), (46 << 9) | (11 << 5) | 30);
});

test("findMsix: the version's package, by itself or in its folder", (t) => {
  const dir = mkdtempSync(join(tmpdir(), "keymotion-submit-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  const file = join(dir, "Keymotion_1.2.0.0_x64.msix");
  writeFileSync(file, "x");
  assert.equal(findMsix(dir, "1.2.0"), file);
  assert.equal(findMsix(file, "1.2.0"), file);
  assert.throws(() => findMsix(dir, "1.3.0"), /Keymotion_1\.3\.0\.0_x64\.msix is not in .* \(found: Keymotion_1\.2\.0\.0_x64\.msix\)/);
  assert.throws(() => findMsix(file, "1.3.0"), /is not Keymotion_1\.3\.0\.0_x64\.msix/);
  assert.throws(() => findMsix(join(dir, "nope"), "1.2.0"), /not found/);
});

test("What's new: one file per listing language, written, within the Store's limit", (t) => {
  assert.deepEqual(STORE_LANGUAGES, ["en", "tr"]);
  assert.match(whatsNewSkeleton("1.2.0", "tr"), /^TODO\(release\): What's new in Keymotion 1\.2\.0 .*in Turkish\./);
  assert.match(whatsNewProblem(whatsNewSkeleton("1.2.0", "en")), /still the skeleton/);
  assert.equal(whatsNewProblem(" \r\n"), "is empty");
  assert.equal(whatsNewProblem("ş".repeat(WHATS_NEW_MAX)), null);
  assert.match(whatsNewProblem("ş".repeat(WHATS_NEW_MAX + 1)), /1501 characters/);

  const dir = mkdtempSync(join(tmpdir(), "keymotion-whatsnew-"));
  t.after(() => rmSync(dir, { recursive: true, force: true }));
  mkdirSync(join(dir, "store", "whats-new"), { recursive: true });
  writeFileSync(join(dir, "store", "whats-new", "1.2.0.en.txt"), "Line one.\r\nLine two.\r\n");
  const r = readWhatsNew(dir, "1.2.0");
  assert.deepEqual(r.notes, { en: "Line one.\nLine two." });
  assert.match(r.problems[0], /^store\/whats-new\/1\.2\.0\.tr\.txt is missing/);
});

test("CLI: without the secrets it skips the submission; with only some it fails", () => {
  const env = { ...process.env, GITHUB_ACTIONS: "", GITHUB_STEP_SUMMARY: "", GITHUB_OUTPUT: "" };
  for (const k of ["STORE_TENANT_ID", "STORE_CLIENT_ID", "STORE_CLIENT_SECRET"]) delete env[k];
  const run = (args, extra = {}) => spawnSync(process.execPath, [script, ...args], { encoding: "utf8", env: { ...env, ...extra } });

  const skip = run(["--tag", "v1.2.0", "--msix", "nowhere"]);
  assert.equal(skip.status, 0, skip.stdout + skip.stderr);
  assert.match(skip.stdout, /are not set: nothing was submitted\. Upload the \.msix in Partner Center by hand\./);

  const some = run(["--tag", "v1.2.0", "--msix", "nowhere"], { STORE_TENANT_ID: "t" });
  assert.equal(some.status, 1);
  assert.match(some.stdout, /STORE_CLIENT_ID, STORE_CLIENT_SECRET not set/);

  // The status can't be read without them, and a pre-release is never submitted.
  assert.equal(run(["--status", "--tag", "v1.2.0"]).status, 1);
  assert.match(run(["--tag", "v1.2.0-rc.1"]).stdout, /not a pre-release/);
});
