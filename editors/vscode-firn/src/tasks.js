const vscode = require("vscode");
const settings = require("./settings");
const { runTarget } = require("./project");

const TASK_TYPE = "firn";
const COMMANDS = [
  { command: "build", group: vscode.TaskGroup.Build },
  { command: "test", group: vscode.TaskGroup.Test },
  { command: "check", group: undefined },
  { command: "run", group: undefined },
  { command: "fmt", group: undefined },
];

function defaultTarget() {
  const editor = vscode.window.activeTextEditor;
  if (editor && editor.document.languageId === "firn") return runTarget(editor.document.uri.fsPath);
  return settings.workspaceRoot();
}

function argsFor(definition) {
  const target = definition.target || defaultTarget();
  if (definition.command === "fmt") return ["fmt", "-c", ...(target ? [target] : [])];
  return [definition.command, ...(target ? [target] : [])];
}

function makeTask(definition, scope) {
  const execution = new vscode.ShellExecution(settings.firnCommand(), argsFor(definition), {
    env: settings.toolEnv(),
    cwd: settings.workspaceRoot(),
  });
  const task = new vscode.Task(definition, scope || vscode.TaskScope.Workspace, `firn ${definition.command}`, TASK_TYPE, execution, "$firnc");
  return task;
}

function register(context) {
  const provider = {
    provideTasks() {
      return COMMANDS.map(({ command, group }) => {
        const task = makeTask({ type: TASK_TYPE, command });
        if (group) task.group = group;
        return task;
      });
    },
    resolveTask(task) {
      const definition = task.definition;
      if (!definition || !definition.command) return undefined;
      return makeTask(definition, task.scope);
    },
  };
  context.subscriptions.push(vscode.tasks.registerTaskProvider(TASK_TYPE, provider));
}

module.exports = { register };
