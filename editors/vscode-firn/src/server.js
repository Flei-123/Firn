const vscode = require("vscode");
const cp = require("child_process");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");
const settings = require("./settings");

const DOCTOR_TIMEOUT_MS = 120000;

let client;
let output;

function findFormatter() {
  const configured = settings.setting("formatterPath");
  if (configured) return Promise.resolve(configured);
  return new Promise((resolve) => {
    cp.execFile(
      settings.firnCommand(),
      ["doctor"],
      { env: settings.toolEnv(), timeout: DOCTOR_TIMEOUT_MS, shell: process.platform === "win32" },
      (error, stdout) => {
        const match = !error && /^fmt\s+(.+)$/m.exec(stdout || "");
        resolve(match ? match[1].trim() : undefined);
      }
    );
  });
}

async function startServer(context) {
  if (!settings.setting("server.enabled")) return;
  output = output || vscode.window.createOutputChannel("Firn Language Server");
  const formatter = await findFormatter();
  const env = settings.toolEnv(formatter ? { FIRNFMT: formatter } : {});
  const command = settings.compilerCommand();
  const serverOptions = {
    run: { command, args: ["--lsp"], transport: TransportKind.stdio, options: { env } },
    debug: { command, args: ["--lsp"], transport: TransportKind.stdio, options: { env } },
  };
  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "firn" }],
    outputChannel: output,
  };
  client = new LanguageClient("firn", "Firn", serverOptions, clientOptions);
  context.subscriptions.push({ dispose: () => stopServer() });
  try {
    await client.start();
  } catch (error) {
    vscode.window.showWarningMessage(
      `Firn language server did not start (${error.message}). Set "firn.compilerPath" to your firnc.`
    );
  }
}

async function stopServer() {
  if (client) {
    const stopping = client;
    client = undefined;
    await stopping.stop().catch(() => undefined);
  }
}

async function restartServer(context) {
  await stopServer();
  await startServer(context);
}

module.exports = { startServer, stopServer, restartServer };
