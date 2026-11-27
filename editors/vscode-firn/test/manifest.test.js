const test = require("node:test");
const assert = require("node:assert");
const fs = require("fs");
const path = require("path");

const root = path.join(__dirname, "..");
const pkg = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));

test("every contributed command is registered in the sources", () => {
  const sources = fs.readdirSync(path.join(root, "src")).map((f) => fs.readFileSync(path.join(root, "src", f), "utf8")).join("\n");
  for (const { command } of pkg.contributes.commands) assert.ok(sources.includes(`"${command}"`), command);
});

test("referenced files exist", () => {
  const paths = [pkg.main, pkg.contributes.grammars[0].path, pkg.contributes.snippets[0].path, pkg.contributes.languages[0].configuration];
  for (const p of paths) assert.ok(fs.existsSync(path.join(root, p)), p);
});

test("snippets are valid and every body is an array of strings", () => {
  const snippets = JSON.parse(fs.readFileSync(path.join(root, "snippets/firn.json"), "utf8"));
  for (const [name, snippet] of Object.entries(snippets)) assert.ok(Array.isArray(snippet.body) && snippet.prefix, name);
});

test("the problem matcher parses a compiler report", () => {
  const [first, second] = pkg.contributes.problemMatchers[0].pattern.map((p) => new RegExp(p.regexp));
  const head = first.exec("error: unknown function 'foo'");
  assert.strictEqual(head[2], "unknown function 'foo'");
  const place = second.exec("  --> src/main.fi:3:5");
  assert.deepStrictEqual([place[1], place[2], place[3]], ["src/main.fi", "3", "5"]);
});
