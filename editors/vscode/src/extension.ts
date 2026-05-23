import { workspace, ExtensionContext } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;

export function activate(_context: ExtensionContext): void {
  const config = workspace.getConfiguration("wcl");
  const serverPath = config.get<string>("serverPath", "wcl");

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
