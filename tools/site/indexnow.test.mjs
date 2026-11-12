// Tests for indexnow.mjs. Run: node --test "tools/site/*.test.mjs"

import assert from "node:assert/strict";
import { test } from "node:test";

import { HOST, KEY, pageUrl, requestBody, sitemapLocs } from "./indexnow.mjs";

const SITE = "https://cebersaban44-rgb.github.io/keymotion/";

test("pageUrl: content files map to their page addresses", () => {
  assert.equal(pageUrl("site/src/content/docs/index.mdx"), SITE);
  assert.equal(pageUrl("site/src/content/docs/tr/index.mdx"), `${SITE}tr/`);
  assert.equal(pageUrl("site/src/content/docs/guide/install.mdx"), `${SITE}guide/install/`);
  assert.equal(pageUrl("site/src/content/docs/tr/help/faq.md"), `${SITE}tr/help/faq/`);
  // A page whose name only ends in "index" keeps it.
  assert.equal(pageUrl("site/src/content/docs/guide/reindex.mdx"), `${SITE}guide/reindex/`);
});

test("pageUrl: anything else is not a page", () => {
  assert.equal(pageUrl("site/astro.config.mjs"), null);
  assert.equal(pageUrl("site/src/content/docs/images/a.png"), null);
  assert.equal(pageUrl("docs/index.md"), null);
  assert.equal(pageUrl(""), null);
});

test("sitemapLocs: the addresses of a sitemap and a sitemap index", () => {
  const index = `<?xml version="1.0"?><sitemapindex><sitemap><loc>${SITE}sitemap-0.xml</loc></sitemap></sitemapindex>`;
  assert.deepEqual(sitemapLocs(index), [`${SITE}sitemap-0.xml`]);
  const sitemap = `<urlset><url><loc>${SITE}</loc><xhtml:link href="${SITE}tr/"/></url><url><loc>\n ${SITE}tr/ </loc></url></urlset>`;
  assert.deepEqual(sitemapLocs(sitemap), [SITE, `${SITE}tr/`]);
});

test("requestBody: host, key, key file at the host root and the list", () => {
  assert.deepEqual(requestBody([SITE]), {
    host: HOST,
    key: KEY,
    keyLocation: `https://${HOST}/${KEY}.txt`,
    urlList: [SITE],
  });
  assert.match(KEY, /^[0-9a-f]{32}$/);
  assert.throws(() => requestBody(["https://example.com/"]), /not a page of the site/);
  assert.throws(() => requestBody(Array(10_001).fill(SITE)), /at most 10,000/);
});
