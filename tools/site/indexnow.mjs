#!/usr/bin/env node
// Tells the IndexNow search engines (Bing, Yandex, Seznam, Naver and the
// others that share submissions) which pages of the documentation site
// changed. .github/workflows/pages.yml runs it right after a publish.
//
// Which pages:
//   - with --since <commit>: the pages whose files under
//     site/src/content/docs changed since that commit (the last successful
//     publish), deleted pages included, so search engines drop them;
//   - without it (the first publish): every page in the live sitemap.
//
// IndexNow checks the key file before it accepts a list. The file is at the
// root of the host, in the cebersaban44-rgb/cebersaban44-rgb.github.io
// repository, because Bing expects it there. The key is public by design.
//
// Usage:
//   node tools/site/indexnow.mjs [--since <commit>] [--dry-run]

import { execFileSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";

export const HOST = "cebersaban44-rgb.github.io";
export const KEY = "39262f4a15b6285193e28376bef4d3d7";
const SITE = `https://${HOST}/keymotion/`;
const CONTENT = "site/src/content/docs/";
const ENDPOINT = "https://api.indexnow.org/indexnow";

/** The page address of a file under site/src/content/docs, or null. */
export function pageUrl(file) {
  if (!file.startsWith(CONTENT) || !/\.mdx?$/.test(file)) return null;
  let path = file.slice(CONTENT.length).replace(/\.mdx?$/, "");
  if (path === "index") path = "";
  else if (path.endsWith("/index")) path = path.slice(0, -"/index".length);
  return path ? `${SITE}${path}/` : SITE;
}

/** The <loc> addresses in a sitemap or sitemap index. */
export const sitemapLocs = (xml) => [...xml.matchAll(/<loc>\s*([^<\s]+)\s*<\/loc>/g)].map((m) => m[1]);

/** The IndexNow request body for a list of page addresses. */
export function requestBody(urls) {
  if (urls.length > 10_000) throw new Error(`IndexNow takes at most 10,000 addresses, got ${urls.length}`);
  for (const url of urls) {
    if (!url.startsWith(SITE)) throw new Error(`not a page of the site: ${url}`);
  }
  return { host: HOST, key: KEY, keyLocation: `https://${HOST}/${KEY}.txt`, urlList: urls };
}

const git = (...args) => execFileSync("git", args, { encoding: "utf8" });

function changedPages(since) {
  const files = git("diff", "--name-only", "--no-renames", since, "HEAD", "--", CONTENT).split("\n");
  return [...new Set(files.map(pageUrl).filter(Boolean))];
}

async function livePages() {
  const get = async (url) => {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`${url}: HTTP ${res.status}`);
    return res.text();
  };
  const pages = [];
  for (const sitemap of sitemapLocs(await get(`${SITE}sitemap-index.xml`))) {
    pages.push(...sitemapLocs(await get(sitemap)));
  }
  return pages;
}

async function main() {
  const { values: opt } = parseArgs({
    options: { since: { type: "string" }, "dry-run": { type: "boolean", default: false } },
  });
  let urls;
  if (opt.since) {
    try {
      git("cat-file", "-e", `${opt.since}^{commit}`);
      urls = changedPages(opt.since);
      console.log(`Pages changed since ${opt.since.slice(0, 7)}: ${urls.length}.`);
    } catch {
      console.log(`${opt.since} isn't in this clone; sending every page instead.`);
    }
  }
  urls ??= await livePages();
  if (urls.length === 0) {
    console.log("No page changed, so there is nothing to send.");
    return;
  }
  for (const url of urls) console.log(`  ${url}`);
  const body = requestBody(urls);
  if (opt["dry-run"]) {
    console.log(`Dry run: would send ${urls.length} addresses to ${ENDPOINT}.`);
    return;
  }
  const res = await fetch(ENDPOINT, {
    method: "POST",
    headers: { "Content-Type": "application/json; charset=utf-8" },
    body: JSON.stringify(body),
  });
  // 200: accepted; 202: accepted, the key is still being checked.
  if (res.status !== 200 && res.status !== 202) {
    throw new Error(`IndexNow answered HTTP ${res.status}: ${(await res.text()).slice(0, 300)}`);
  }
  console.log(`Sent ${urls.length} addresses to IndexNow (HTTP ${res.status}).`);
}

const isMain = process.argv[1] && pathToFileURL(realpathSync(process.argv[1])).href === import.meta.url;
if (isMain) {
  main().catch((e) => {
    console.log(`${process.env.GITHUB_ACTIONS ? "::warning::" : ""}${e.message}`);
    process.exit(1);
  });
}
