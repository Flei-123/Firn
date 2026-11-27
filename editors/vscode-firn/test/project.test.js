const test = require("node:test");
const assert = require("node:assert");
const path = require("path");
const { findProjectDir, runTarget } = require("../src/project");

const exists = (set) => (p) => set.has(p);

test("finds the nearest firn.package upwards", () => {
  const manifest = path.join("/w/proj", "firn.package");
  assert.strictEqual(findProjectDir(path.join("/w/proj/src/a/b.fi"), exists(new Set([manifest]))), "/w/proj");
});

test("a lone file has no project", () => {
  assert.strictEqual(findProjectDir("/tmp/x.fi", exists(new Set())), undefined);
});

test("run target is the project directory, else the file", () => {
  const manifest = path.join("/w/proj", "firn.package");
  assert.strictEqual(runTarget("/w/proj/src/main.fi", exists(new Set([manifest]))), "/w/proj");
  assert.strictEqual(runTarget("/tmp/x.fi", exists(new Set())), "/tmp/x.fi");
});
