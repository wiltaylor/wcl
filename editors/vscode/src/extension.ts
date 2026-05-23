import { existsSync } from "fs";
import { homedir } from "os";
import * as path from "path";
import { workspace, ExtensionContext } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

/// Resolve `wcl.serverPath` to an absolute binary path when the user
/// has left the setting at its default (`"wcl"`). VS Code launched
/// from a GUI app launcher typically doesn't inherit the shell's
/// PATH, so a bare `wcl` would fail with `ENOENT` even when the
/// binary lives at `~/.cargo/bin/wcl`. Probing common install
/// locations covers the cargo-install path out of the box; users
/// who set `wcl.serverPath` explicitly keep full control.
function resolveServerPath(configured: string): string {
  if (configured !== "wcl") {
    return configured;
  }
  const home = homedir();
  const candidates = [
    path.join(home, ".cargo", "bin", "wcl"),
    "/usr/local/bin/wcl",
    "/opt/homebrew/bin/wcl",
  ];
  for (const c of candidates) {
    if (existsSync(c)) {
      return c;
    }
  }
  return configured;
}

export function activate(_context: ExtensionContext): void {
  const config = workspace.getConfiguration("wcl");
  const serverPath = resolveServerPath(config.get<string>("serverPath", "wcl"));

  // The `wcl lsp` subcommand speaks LSP over stdio.
  const serverOptions: ServerOptions = {
    run: { command: serverPath, args: ["lsp"], transport: TransportKind.stdio },
    debug: { command: serverPath, args: ["lsp"], transport: TransportKind.stdio },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "wcl" }],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/*.wcl"),
    },
  };

  client = new LanguageClient(
    "wcl",
    "WCL Language Server",
    serverOptions,
    clientOptions,
  );
  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  return client?.stop();
}
