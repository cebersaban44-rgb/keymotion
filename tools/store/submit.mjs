#!/usr/bin/env node
// Sends a release's Microsoft Store package for certification, through the
// Store's submission API (docs site: developers/microsoft-store). store.yml's
// Submit job runs it on a release tag, after the .msix is built.
//
//   node tools/store/submit.mjs --tag v1.2.0 --msix <the .msix, or its folder>
//       1. signs in with Microsoft Entra (client credentials);
//       2. stops, changing nothing, when the app has a pending submission
//          (one made in Partner Center, or by an earlier run): only one can
//          be pending, and this script never deletes one;
//       3. makes a new submission (a copy of the last published one), with
//          the new package (the old one PendingDelete), "What's new" in
//          every listing language from store/whats-new/<version>.<lang>.txt,
//          and targetPublishMode "Immediate", set every time (a new
//          submission copies the last one's, which may have waited for
//          "Publish now"): the version publishes by itself as soon as it
//          passes certification (owner's decision, 2026-11-25);
//       4. uploads the package (a ZIP) to the submission's fileUploadUrl,
//          commits it and waits while the commit runs.
//   node tools/store/submit.mjs --status --tag v1.2.0
//       Only reads: the status of the submission that carries the version's
//       package (Certification, Release, Published, …). publish.yml shows
//       it beside the GitHub release; nothing waits for it.
//
// The secrets STORE_TENANT_ID, STORE_CLIENT_ID and STORE_CLIENT_SECRET (a
// Microsoft Entra app with the Manager role in Partner Center) come from the
// microsoft-store environment. Without them a submission (or a status) is
// skipped with a notice, and the package is uploaded in Partner Center by
// hand, set to publish as soon as it passes certification.
//
// Microsoft's msstore tool is not used: it deletes a pending submission.
// A submission made here must not be edited in Partner Center while this
// script may still change it: the API loses access to it.

import { createHash } from "node:crypto";
import { appendFileSync, existsSync, readFileSync, readdirSync, realpathSync, statSync } from "node:fs";
import { basename, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { crc32 } from "node:zlib";

import { storeVersion } from "./msix.mjs";
import { readWhatsNew } from "./whats-new.mjs";

const root = fileURLToPath(new URL("../..", import.meta.url));

/** Keymotion's Store ID (Partner Center › the app › Product identity). */
export const STORE_APP_ID = "9NL03GDZZ0ZS";

const RESOURCE = "https://manage.devcenter.microsoft.com";
const API = `${RESOURCE}/v1.0/my/applications`;
const tokenUrl = (tenant) => `https://login.microsoftonline.com/${encodeURIComponent(tenant)}/oauth2/token`;

export const SECRET_NAMES = ["STORE_TENANT_ID", "STORE_CLIENT_ID", "STORE_CLIENT_SECRET"];

/** The three secrets, null when none is set; an error when only some are. */
export function storeSecrets(env) {
  const set = SECRET_NAMES.filter((k) => env[k]?.trim());
  if (!set.length) return null;
  const missing = SECRET_NAMES.filter((k) => !set.includes(k));
  if (missing.length) {
    throw new Error(`${missing.join(", ")} not set, while ${set.join(", ")} is: set all three in the microsoft-store environment`);
  }
  return { tenantId: env.STORE_TENANT_ID.trim(), clientId: env.STORE_CLIENT_ID.trim(), clientSecret: env.STORE_CLIENT_SECRET.trim() };
}

async function readBody(res) {
  const text = await res.text();
  if (!text) return null;
  try {
    return JSON.parse(text);
  } catch {
    return { message: text.slice(0, 300) };
  }
}

const sleepMs = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * The Store API for one app. `fetch`, `sleep` and `now` can be replaced
 * (tests). GET requests are tried again on 429 and server errors; nothing
 * else is, so a submission is never made twice.
 */
export function storeClient({ tenantId, clientId, clientSecret, appId = STORE_APP_ID, fetch = globalThis.fetch, sleep = sleepMs, now = Date.now }) {
  let token = null;
  let expires = 0;

  async function accessToken() {
    if (token && now() < expires - 60_000) return token;
    const res = await fetch(tokenUrl(tenantId), {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ grant_type: "client_credentials", client_id: clientId, client_secret: clientSecret, resource: RESOURCE }).toString(),
    });
    const body = await readBody(res);
    if (!res.ok || !body?.access_token) {
      const why = (body?.error_description?.split(/\r?\n/)[0] ?? body?.error ?? body?.message ?? "no token").replace(/\.$/, "");
      throw new Error(
        `Microsoft Entra sign-in failed (HTTP ${res.status}): ${why}. Check STORE_TENANT_ID, STORE_CLIENT_ID and STORE_CLIENT_SECRET; the key may have expired.`,
      );
    }
    token = body.access_token;
    expires = now() + Number(body.expires_in ?? 3600) * 1000;
    return token;
  }

  async function request(method, path, json) {
    for (let attempt = 1; ; attempt++) {
      const res = await fetch(`${API}/${appId}${path}`, {
        method,
        headers: { Authorization: `Bearer ${await accessToken()}`, ...(json ? { "Content-Type": "application/json" } : {}) },
        ...(json ? { body: JSON.stringify(json) } : {}),
      });
      if (method === "GET" && (res.status === 429 || res.status >= 500) && attempt < 4) {
        await res.text().catch(() => {});
        await sleep(attempt * 5000);
        continue;
      }
      const body = await readBody(res);
      if (!res.ok) {
        const why = [body?.code, body?.message ?? body?.error?.message].filter(Boolean).join(": ") || "no details";
        throw new Error(`Store API ${method} ${path || "/"}: HTTP ${res.status} (${why})`);
      }
      return body;
    }
  }

  return {
    appId,
    getApp: () => request("GET", ""),
    getSubmission: (id) => request("GET", `/submissions/${id}`),
    createSubmission: () => request("POST", "/submissions"),
    updateSubmission: (id, submission) => request("PUT", `/submissions/${id}`, submission),
    commit: (id) => request("POST", `/submissions/${id}/commit`),
    status: (id) => request("GET", `/submissions/${id}/status`),
    /** The package ZIP to the submission's Azure blob (a SAS URL, never printed). */
    async upload(url, bytes) {
      const res = await fetch(url, {
        method: "PUT",
        headers: { "x-ms-blob-type": "BlockBlob", "Content-Type": "application/zip" },
        body: bytes,
      });
      if (!res.ok) {
        const code = /<Code>([^<]*)<\/Code>/.exec(await res.text())?.[1] ?? "no details";
        throw new Error(`uploading the package failed: HTTP ${res.status} (${code})`);
      }
    },
  };
}

/**
 * Which listing gets which language's notes: a listing "en-us" takes "en".
 * Every listing needs notes (or it would keep the last version's) and every
 * language a listing.
 */
export function matchListings(listingKeys, languages) {
  const pairs = [];
  const missing = [];
  for (const key of listingKeys) {
    const k = key.toLowerCase();
    const lang = languages.find((l) => k === l.toLowerCase() || k.startsWith(`${l.toLowerCase()}-`));
    if (lang) pairs.push([key, lang]);
    else missing.push(key);
  }
  const unused = languages.filter((l) => !pairs.some(([, lang]) => lang === l));
  return { pairs, missing, unused };
}

/**
 * The submission to PUT: the old packages PendingDelete, the new one
 * PendingUpload, "What's new" in each listing, and "Immediate" publishing
 * (as soon as it passes certification). The input is not changed.
 */
export function prepareSubmission(submission, { packageFile, notes }) {
  const s = structuredClone(submission);
  const listings = s.listings ?? {};
  const { pairs, missing, unused } = matchListings(Object.keys(listings), Object.keys(notes));
  if (missing.length) {
    throw new Error(
      `the Store listing has ${missing.join(", ")}, which has no What's new file: add its language to STORE_LANGUAGES (tools/store/whats-new.mjs) and write the file`,
    );
  }
  if (unused.length) throw new Error(`What's new in ${unused.join(", ")} has no Store listing to go to`);
  for (const [key, lang] of pairs) {
    if (!listings[key].baseListing) throw new Error(`the Store listing ${key} has no baseListing`);
    listings[key].baseListing.releaseNotes = notes[lang];
  }
  s.applicationPackages = [
    ...(s.applicationPackages ?? []).map((p) => ({ ...p, fileStatus: "PendingDelete" })),
    { fileName: packageFile, fileStatus: "PendingUpload", minimumDirectXVersion: "None", minimumSystemRam: "None" },
  ];
  s.targetPublishMode = "Immediate";
  return s;
}

/** A ZIP with one file, stored as it is (an .msix is compressed already). */
export function zipOne(name, data, date = new Date()) {
  if (data.length >= 0xffffffff) throw new Error(`${name} is too large for a ZIP without ZIP64`);
  const fileName = Buffer.from(name, "utf8");
  const crc = crc32(data);
  const time = (date.getHours() << 11) | (date.getMinutes() << 5) | (date.getSeconds() >> 1);
  const day = ((date.getFullYear() - 1980) << 9) | ((date.getMonth() + 1) << 5) | date.getDate();

  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0);
  local.writeUInt16LE(20, 4); // version needed
  local.writeUInt16LE(0, 6); // flags
  local.writeUInt16LE(0, 8); // stored
  local.writeUInt16LE(time, 10);
  local.writeUInt16LE(day, 12);
  local.writeUInt32LE(crc, 14);
  local.writeUInt32LE(data.length, 18);
  local.writeUInt32LE(data.length, 22);
  local.writeUInt16LE(fileName.length, 26);

  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0);
  central.writeUInt16LE(20, 4); // made by
  central.writeUInt16LE(20, 6); // version needed
  central.writeUInt16LE(0, 10); // stored
  central.writeUInt16LE(time, 12);
  central.writeUInt16LE(day, 14);
  central.writeUInt32LE(crc, 16);
  central.writeUInt32LE(data.length, 20);
  central.writeUInt32LE(data.length, 24);
  central.writeUInt16LE(fileName.length, 28);
  central.writeUInt32LE(0, 42); // the local header's offset

  const centralOffset = local.length + fileName.length + data.length;
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(1, 8);
  end.writeUInt16LE(1, 10);
  end.writeUInt32LE(central.length + fileName.length, 12);
  end.writeUInt32LE(centralOffset, 16);
  return Buffer.concat([local, fileName, data, central, fileName, end]);
}

/** The errors and warnings of a status, one line each. */
export function statusDetails(details) {
  const lines = [];
  for (const kind of ["errors", "warnings"]) {
    for (const d of details?.[kind] ?? []) lines.push(`${kind === "errors" ? "Error" : "Warning"} ${d.code ?? ""}: ${d.details ?? ""}`.trim());
  }
  for (const r of details?.certificationReports ?? []) if (r.reportUrl) lines.push(`Certification report: ${r.reportUrl}`);
  return lines;
}

/** A status the submission doesn't come back from. */
export const failed = (status) => /Failed$/.test(status ?? "") || status === "Canceled";

/**
 * Submits `msix` ({ name, bytes }) with `notes` ({ en: "...", ... }):
 * `{ id, status }`. Throws, having changed nothing, when a submission is
 * pending; any later failure leaves the new submission as it is, never
 * deleted.
 */
export async function submit({ client, msix, notes, log = console.log, sleep = sleepMs, pollMs = 30_000, timeoutMs = 45 * 60_000, now = Date.now }) {
  const app = await client.getApp();
  const pending = app?.pendingApplicationSubmission?.id;
  if (pending) {
    throw new Error(
      `submission ${pending} is pending in Partner Center, so nothing was changed (only one can be pending, and this script never deletes one). ` +
        "Finish or delete it in Partner Center, then run the Submit job again; or upload this package by hand.",
    );
  }
  const lastId = app?.lastPublishedApplicationSubmission?.id;
  if (!lastId) throw new Error("the app has no published submission to start from: make the first one in Partner Center");
  // The listings are checked against the What's new files before anything
  // is made: a new submission copies them from the last published one.
  prepareSubmission(await client.getSubmission(lastId), { packageFile: msix.name, notes });

  const created = await client.createSubmission();
  try {
    if (!created?.id || !created.fileUploadUrl) throw new Error("the Store made a submission without an id or an upload address");
    log(`Made submission ${created.id} (a copy of ${lastId}).`);
    const updated = await client.updateSubmission(created.id, prepareSubmission(created, { packageFile: msix.name, notes }));
    if (updated?.targetPublishMode && updated.targetPublishMode !== "Immediate") {
      throw new Error(
        `the submission says targetPublishMode ${updated.targetPublishMode}, not Immediate: it would not publish by itself when it passes certification`,
      );
    }
    log(`Set the package ${msix.name}, What's new (${Object.keys(notes).join(", ")}) and publishing as soon as it passes certification.`);
    await client.upload(created.fileUploadUrl, zipOne(msix.name, msix.bytes));
    log(`Uploaded ${msix.name} (${msix.bytes.length} bytes).`);

    let { status, statusDetails: details } = (await client.commit(created.id)) ?? {};
    log(`Committed: ${status}.`);
    const deadline = now() + timeoutMs;
    while (status === "CommitStarted") {
      if (now() > deadline) throw new Error(`the commit was still running after ${Math.round(timeoutMs / 60_000)} minutes`);
      await sleep(pollMs);
      ({ status, statusDetails: details } = await client.status(created.id));
    }
    for (const line of statusDetails(details)) log(line);
    if (failed(status)) throw new Error(`the submission is ${status}`);
    log(`Submission ${created.id}: ${status}.`);
    return { id: created.id, status };
  } catch (e) {
    const which = created?.id ? `Submission ${created.id}` : "A new submission (the Store gave no id)";
    e.message +=
      `\n${which} was made and is left as it is (this script never deletes one). ` +
      "In Partner Center, finish it there, or delete it and run the Submit job again.";
    throw e;
  }
}

/** Whether a submission carries the package of Store version `pkg` (1.2.0.0). */
const carries = (submission, pkg) =>
  (submission?.applicationPackages ?? []).some(
    (p) => p.fileStatus !== "PendingDelete" && (p.version === pkg || (p.fileName ?? "").includes(`_${pkg}_`)),
  );

/**
 * Only reads: `{ id, status, details, pending }` of the submission (pending
 * or last published) that carries `version`'s package; status "NotFound"
 * when neither does.
 */
export async function storeStatus({ client, version }) {
  const pkg = storeVersion(version);
  const app = await client.getApp();
  for (const [pending, ref] of [[true, app?.pendingApplicationSubmission], [false, app?.lastPublishedApplicationSubmission]]) {
    if (!ref?.id) continue;
    const s = await client.getSubmission(ref.id);
    if (carries(s, pkg)) return { id: s.id ?? ref.id, status: s.status, details: s.statusDetails, pending };
  }
  return { id: null, status: "NotFound", details: null, pending: false };
}

/** The .msix of `version`: `msix` itself, or the one in that folder. */
export function findMsix(msix, version) {
  const expected = `Keymotion_${storeVersion(version)}_x64.msix`;
  if (!existsSync(msix)) throw new Error(`${msix} not found`);
  const path = statSync(msix).isDirectory() ? join(msix, expected) : msix;
  if (basename(path) !== expected) throw new Error(`${basename(path)} is not ${expected}, the package of ${version}`);
  if (!existsSync(path)) {
    const found = readdirSync(msix).filter((f) => f.endsWith(".msix"));
    throw new Error(`${expected} is not in ${msix} (found: ${found.join(", ") || "no .msix"})`);
  }
  return path;
}

function summary(lines) {
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `${lines.join("\n")}\n`);
}

async function main() {
  const { values: opt } = parseArgs({
    options: { tag: { type: "string" }, msix: { type: "string" }, status: { type: "boolean", default: false } },
  });
  const m = /^v(\d+\.\d+\.\d+)$/.exec(opt.tag ?? "");
  if (!m) throw new Error(`--tag takes a release tag such as v1.2.0 (not a pre-release), got ${JSON.stringify(opt.tag)}`);
  const version = m[1];
  const ci = !!process.env.GITHUB_ACTIONS;

  const secrets = storeSecrets(process.env);
  if (!secrets) {
    const names = SECRET_NAMES.join(", ");
    if (opt.status) {
      console.log(`${ci ? "::notice::" : ""}${names} are not set (microsoft-store environment): the Store was not asked.`);
      summary([`### Keymotion ${version} in the Microsoft Store: not asked`, "", `The secrets ${names} are not set, so the Store can't be asked.`]);
      return;
    }
    console.log(
      `${ci ? "::notice::" : ""}${names} are not set: nothing was submitted. ` +
        "Upload the .msix in Partner Center by hand, set to publish as soon as it passes certification.",
    );
    summary([
      "### Not submitted to the Store",
      "",
      `The secrets ${names} are not set. Upload the package in Partner Center by hand: download the .msix from this run's artifacts, ` +
        "start a new submission, upload it under Packages (remove the old one), and fill in What's new from store/whats-new.",
      "",
      "Under Submission options, choose to publish the submission **as soon as it passes certification**, not " +
        "\"Don't publish this submission until I select Publish now\": a new submission starts as a copy of the last one, so check it.",
    ]);
    return;
  }
  const client = storeClient(secrets);

  if (opt.status) {
    const s = await storeStatus({ client, version });
    const where = s.id ? `submission ${s.id} (${s.pending ? "pending" : "last published"})` : "no submission";
    console.log(`Keymotion ${version} in the Store: ${s.status}, ${where}.`);
    for (const line of statusDetails(s.details)) console.log(line);
    if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `status=${s.status}\n`);
    summary([`### Keymotion ${version} in the Microsoft Store: ${s.status}`, "", `${where[0].toUpperCase()}${where.slice(1)}.`, ...statusDetails(s.details).map((l) => `- ${l}`)]);
    return;
  }

  if (!opt.msix) throw new Error("--msix is required (the .msix, or the folder it is in)");
  const path = findMsix(opt.msix, version);
  const { notes, problems } = readWhatsNew(root, version);
  if (problems.length) throw new Error(`What's new is not ready:\n${problems.map((p) => `  - ${p}`).join("\n")}`);
  const bytes = readFileSync(path);
  console.log(`${basename(path)}: ${bytes.length} bytes, SHA-256 ${createHash("sha256").update(bytes).digest("hex")}.`);
  const result = await submit({ client, msix: { name: basename(path), bytes }, notes });
  summary([
    `### Keymotion ${version} is submitted to the Microsoft Store`,
    "",
    `Submission ${result.id}: ${result.status}. It publishes by itself when it passes certification ` +
      "(Microsoft allows up to three working days; usually within a day). Nothing to select in Partner Center.",
    "",
    "The GitHub release doesn't wait for it: publish it with the **Publish a release** workflow, run from this tag, once the install test has passed.",
  ]);
}

const isMain = process.argv[1] && pathToFileURL(realpathSync(process.argv[1])).href === import.meta.url;
if (isMain) {
  main().catch((e) => {
    const msg = e instanceof Error ? e.message : String(e);
    console.log(process.env.GITHUB_ACTIONS ? `::error::${msg.replace(/\n/g, "%0A")}` : msg);
    process.exit(1);
  });
}
