import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function render() {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  return worker.fetch(
    new Request("https://apex-exec.example/", {
      headers: { accept: "text/html" },
    }),
    {
      ASSETS: {
        fetch: async () => new Response("Not found", { status: 404 }),
      },
    },
    {
      waitUntil() {},
      passThroughOnException() {},
    },
  );
}

test("server-renders the engineering leadership site", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);

  const html = await response.text();
  assert.match(html, /Move the Apex/);
  assert.match(html, /inner loop/);
  assert.match(html, /The leadership case/);
  assert.match(html, /Built as infrastructure, not a demo/);
  assert.match(html, /SObject schema \+ SQLite/);
  assert.match(html, /Compatibility posture/);
  assert.match(html, /github\.com\/a-barwick\/apex-exec/);
  assert.doesNotMatch(html, /codex-preview|Your site is taking shape/);
});

test("removes starter assets and publishes site-specific metadata", async () => {
  const [layout, page, packageJson] = await Promise.all([
    readFile(new URL("app/layout.tsx", root), "utf8"),
    readFile(new URL("app/page.tsx", root), "utf8"),
    readFile(new URL("package.json", root), "utf8"),
  ]);

  assert.match(layout, /Apex Exec — Move the Apex inner loop off the org/);
  assert.match(layout, /new URL\("\/og\.png", base\)/);
  assert.match(page, /M7 active/);
  assert.doesNotMatch(packageJson, /react-loading-skeleton/);
  await assert.rejects(access(new URL("app/_sites-preview", root)));
});
