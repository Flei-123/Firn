const test = require("node:test");
const assert = require("node:assert");
const Module = require("module");
const path = require("path");

const config = { path: "", compilerPath: "/opt/firnc", formatterPath: "/opt/firnfmt", libPath: "/opt/lib", optLevel: "dev-fast", saveBeforeRun: true, "server.enabled": true };
const commands = new Map();
const providers = [];
const started = [];

class FakeTask {
  constructor(definition, scope, name, source, execution, matcher) {
    Object.assign(this, { definition, scope, name, source, execution, matcher });
  }
}

const fakeVscode = {
  workspace: {
    getConfiguration: () => ({ get: (name) => config[name] }),
    workspaceFolders: [{ uri: { fsPath: "/ws" } }],
    onDidChangeConfiguration: () => ({ dispose() {} }),
    saveAll: async () => true,
  },
  commands: {
    registerCommand: (id, handler) => (commands.set(id, handler), { dispose() {} }),
    executeCommand: async () => undefined,
  },
  tasks: { registerTaskProvider: (type, provider) => (providers.push({ type, provider }), { dispose() {} }) },
  window: {
    activeTextEditor: undefined,
    terminals: [],
    createOutputChannel: () => ({ appendLine() {} }),
    showWarningMessage() {},
    showInputBox: async () => undefined,
    createTerminal: () => ({ name: "Firn", show() {}, sendText() {} }),
  },
  Task: FakeTask,
  TaskGroup: { Build: "build", Test: "test" },
  TaskScope: { Workspace: 2 },
  ShellExecution: class { constructor(command, args, options) { Object.assign(this, { command, args, options }); } },
  Uri: { file: (p) => ({ fsPath: p }) },
};

class FakeClient {
  constructor(id, name, serverOptions) { this.serverOptions = serverOptions; }
  async start() { started.push(this.serverOptions); }
  async stop() {}
}

const originalLoad = Module._load;
Module._load = function (request, ...rest) {
  if (request === "vscode") return fakeVscode;
  if (request === "vscode-languageclient/node") return { LanguageClient: FakeClient, TransportKind: { stdio: 0 } };
  return originalLoad.call(this, request, ...rest);
};

test("activation registers commands, tasks and starts the server with the configured paths", async () => {
  const { activate } = require("../src/extension");
  const context = { subscriptions: [] };
  await activate(context);
  for (const id of ["firn.run", "firn.build", "firn.test", "firn.check", "firn.formatCheck", "firn.newProject", "firn.restartServer"]) {
    assert.ok(commands.has(id), id);
  }
  assert.strictEqual(started.length, 1);
  assert.strictEqual(started[0].run.command, "/opt/firnc");
  assert.deepStrictEqual(started[0].run.args, ["--lsp"]);
  assert.strictEqual(started[0].run.options.env.FIRNFMT, "/opt/firnfmt");
  assert.strictEqual(started[0].run.options.env.FIRNLIB, "/opt/lib");
});

test("the task provider offers build, test, check, run and fmt", () => {
  const tasks = providers[0].provider.provideTasks();
  assert.deepStrictEqual(tasks.map((t) => t.definition.command), ["build", "test", "check", "run", "fmt"]);
  assert.strictEqual(tasks[0].matcher, "$firnc");
  assert.deepStrictEqual(tasks[4].execution.args.slice(0, 2), ["fmt", "-c"]);
});
