const vscode = require("vscode");
const path = require("path");
const settings = require("./settings");
const { runTarget, findProjectDir } = require("./project");

const TERMINAL_NAME = "Firn";

function activeFile() {
  const editor = vscode.window.activeTextEditor;
  return editor && editor.document.languageId === "firn" ? editor.document : undefined;
}

async function saveIfWanted() {
  if (settings.setting("saveBeforeRun")) await vscode.workspace.saveAll(false);
}

function quote(text) {
  return /[\s"'&|<>()]/.test(text) ? `"${text.replace(/"/g, '\\"')}"` : text;
}

function terminal() {
  const existing = vscode.window.terminals.find((t) => t.name === TERMINAL_NAME);
  if (existing) return existing;
  return vscode.window.createTerminal({ name: TERMINAL_NAME, env: settings.toolEnv() });
}

function targetFor(document) {
  if (document) return runTarget(document.uri.fsPath);
  return settings.workspaceRoot();
}

function workingDirectory(target) {
  if (!target) return undefined;
  return path.extname(target) ? path.dirname(target) : target;
}

async function runFirn(args, target, cwd = workingDirectory(target)) {
  await saveIfWanted();
  const term = terminal();
  term.show(true);
  if (cwd) term.sendText(`cd ${quote(cwd)}`);
  const parts = [settings.firnCommand(), ...args, ...(target ? [target] : [])];
  term.sendText(parts.map(quote).join(" "));
}

function register(context) {
  const add = (id, handler) => context.subscriptions.push(vscode.commands.registerCommand(id, handler));
  add("firn.run", () => runFirn(["run"], targetFor(activeFile())));
  add("firn.build", () => runFirn(["build"], targetFor(activeFile())));
  add("firn.check", () => runFirn(["check"], targetFor(activeFile())));
  add("firn.test", () => {
    const document = activeFile();
    const root = document ? findProjectDir(document.uri.fsPath) : settings.workspaceRoot();
    return runFirn(["test"], undefined, root);
  });
  add("firn.formatCheck", () => {
    const document = activeFile();
    return runFirn(["fmt", "-c"], document ? document.uri.fsPath : undefined);
  });
  add("firn.newProject", async () => {
    const name = await vscode.window.showInputBox({ prompt: "Project name", validateInput: (v) => (/^[A-Za-z_]\w*$/.test(v) ? undefined : "letters, digits, _ ; start with a letter") });
    if (!name) return;
    const parent = settings.workspaceRoot();
    await runFirn(["new", name], undefined, parent);
    if (parent) {
      const uri = vscode.Uri.file(path.join(parent, name));
      vscode.commands.executeCommand("vscode.openFolder", uri, { forceNewWindow: false });
    }
  });
}

module.exports = { register };
