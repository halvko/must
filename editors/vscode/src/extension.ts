import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import {
  Executable,
  LanguageClient,
  LanguageClientOptions,
} from "vscode-languageclient/node";
import { serialized } from "./serial";

let client: LanguageClient | undefined;
let output: vscode.OutputChannel | undefined;
const exclusive = serialized();

const SERVER_NAME = process.platform === "win32" ? "must-lsp.exe" : "must-lsp";

function isExecutable(file: string): boolean {
  try {
    fs.accessSync(file, fs.constants.X_OK);
    return fs.statSync(file).isFile();
  } catch {
    return false;
  }
}

/** `must.serverPath`, else `must-lsp` on PATH, else a workspace's debug build. */
function findServer(): string | undefined {
  const configured = vscode.workspace
    .getConfiguration("must")
    .get<string>("serverPath", "")
    .trim();
  if (configured !== "") {
    return configured;
  }
  const onPath = (process.env.PATH ?? "")
    .split(path.delimiter)
    .filter((dir) => dir !== "")
    .map((dir) => path.join(dir, SERVER_NAME))
    .find(isExecutable);
  if (onPath !== undefined) {
    return onPath;
  }
  return (vscode.workspace.workspaceFolders ?? [])
    .map((folder) =>
      path.join(folder.uri.fsPath, "target", "debug", SERVER_NAME),
    )
    .find(isExecutable);
}

async function startClient(): Promise<void> {
  const command = findServer();
  if (command === undefined) {
    void vscode.window.showErrorMessage(
      "Must: no must-lsp found. Put it on PATH, build it into " +
        "target/debug/ of a workspace folder, or set must.serverPath.",
    );
    return;
  }
  const server: Executable = {
    command,
    args: [],
    options: { env: { ...process.env } },
  };
  const options: LanguageClientOptions = {
    documentSelector: [
      { language: "must", scheme: "file" },
      { language: "must", scheme: "untitled" },
    ],
    outputChannel: output,
  };
  client = new LanguageClient("must", "Must LSP", server, options);
  await client.start();
}

async function stopClient(): Promise<void> {
  const running = client;
  client = undefined;
  try {
    await running?.stop();
  } catch {
    // A client whose server failed to start has nothing to stop.
  }
}

const restartClient = (): Promise<void> =>
  exclusive(async () => {
    await stopClient();
    await startClient();
  });

export async function activate(
  context: vscode.ExtensionContext,
): Promise<void> {
  output = vscode.window.createOutputChannel("Must LSP");
  context.subscriptions.push(
    output,
    vscode.commands.registerCommand("must.restartServer", restartClient),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("must.serverPath")) {
        void restartClient();
      }
    }),
  );
  await exclusive(startClient);
}

export function deactivate(): Promise<void> {
  return exclusive(stopClient);
}
