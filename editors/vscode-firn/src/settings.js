const vscode = require("vscode");
const path = require("path");

const IS_WINDOWS = process.platform === "win32";

function section() {
  return vscode.workspace.getConfiguration("firn");
}

function setting(name) {
  const value = section().get(name);
  return typeof value === "string" ? value.trim() : value;
}

function firnCommand() {
  return setting("path") || (IS_WINDOWS ? "firn.cmd" : "firn");
}

function compilerCommand() {
  return setting("compilerPath") || (IS_WINDOWS ? "firnc.exe" : "firnc");
}

function toolEnv(extra = {}) {
  const env = { ...process.env, ...extra };
  const lib = setting("libPath");
  if (lib) env.FIRNLIB = lib;
  const compiler = setting("compilerPath");
  if (compiler && !env.FIRNC) env.FIRNC = compiler;
  env.FIRN_OPT = setting("optLevel") || "dev-fast";
  return env;
}

function workspaceRoot() {
  const folders = vscode.workspace.workspaceFolders;
  return folders && folders.length ? folders[0].uri.fsPath : undefined;
}

function compilerDir() {
  const compiler = setting("compilerPath");
  return compiler ? path.dirname(compiler) : undefined;
}

module.exports = { setting, firnCommand, compilerCommand, toolEnv, workspaceRoot, compilerDir };
