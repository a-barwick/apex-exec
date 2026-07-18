"use strict";

const vscode = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

let client;
let clientReady;
let coveredDecoration;
let uncoveredDecoration;

function executable() {
  return vscode.workspace
    .getConfiguration("apexExec")
    .get("executable", "apex-exec");
}

function activate(context) {
  const workspace = vscode.workspace.workspaceFolders?.[0]?.uri.fsPath;
  const args = workspace ? ["lsp", workspace] : ["lsp"];
  client = new LanguageClient(
    "apexExec",
    "Apex Exec",
    {
      run: { command: executable(), args, transport: TransportKind.stdio },
      debug: { command: executable(), args, transport: TransportKind.stdio }
    },
    {
      documentSelector: [{ scheme: "file", language: "apex" }],
      synchronize: {
        fileEvents: vscode.workspace.createFileSystemWatcher("**/*.{cls,trigger,apex}")
      }
    }
  );
  clientReady = client.start();
  context.subscriptions.push(client);
  coveredDecoration = vscode.window.createTextEditorDecorationType({
    isWholeLine: true,
    backgroundColor: "rgba(46, 160, 67, 0.12)",
    overviewRulerColor: "rgba(46, 160, 67, 0.8)"
  });
  uncoveredDecoration = vscode.window.createTextEditorDecorationType({
    isWholeLine: true,
    backgroundColor: "rgba(248, 81, 73, 0.15)",
    overviewRulerColor: "rgba(248, 81, 73, 0.9)"
  });
  context.subscriptions.push(coveredDecoration, uncoveredDecoration);
  context.subscriptions.push(
    vscode.commands.registerCommand("apexExec.refreshCoverage", async () => {
      await clientReady;
      const overlays = await client.sendRequest("apex/coverage");
      for (const editor of vscode.window.visibleTextEditors) {
        const overlay = overlays.find(item => item.uri === editor.document.uri.toString());
        const covered = [];
        const uncovered = [];
        for (const line of overlay?.lines ?? []) {
          const range = editor.document.lineAt(line.line).range;
          (line.covered ? covered : uncovered).push(range);
        }
        editor.setDecorations(coveredDecoration, covered);
        editor.setDecorations(uncoveredDecoration, uncovered);
      }
    })
  );
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory("apex-exec", {
      createDebugAdapterDescriptor() {
        return new vscode.DebugAdapterExecutable(executable(), ["dap"]);
      }
    })
  );
}

async function deactivate() {
  if (client) {
    await client.stop();
  }
}

module.exports = { activate, deactivate };
