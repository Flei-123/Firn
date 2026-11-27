const vscode = require("vscode");
const server = require("./server");
const run = require("./run");
const tasks = require("./tasks");

function activate(context) {
  run.register(context);
  tasks.register(context);
  context.subscriptions.push(
    vscode.commands.registerCommand("firn.restartServer", () => server.restartServer(context)),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("firn.compilerPath") || event.affectsConfiguration("firn.libPath") || event.affectsConfiguration("firn.formatterPath") || event.affectsConfiguration("firn.server.enabled")) {
        server.restartServer(context);
      }
    })
  );
  return server.startServer(context);
}

function deactivate() {
  return server.stopServer();
}

module.exports = { activate, deactivate };
