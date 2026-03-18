const vscode = require("vscode");
const path = require("path");
const os = require("os");
const fs = require("fs");
const { LanguageClient } = require("vscode-languageclient/node");

let client;

function findWclBinary(configured) {
  if (path.isAbsolute(configured) && fs.existsSync(configured)) {
    return configured;
  }
  const cargoBin = path.join(os.homedir(), ".cargo", "bin", "wcl");
  if (fs.existsSync(cargoBin)) {
    return cargoBin;
  }
  return configured;
}

function activate(context) {
  const config = vscode.workspace.getConfiguration("wcl");
  const configured = config.get("server.path", "wcl");
  const command = findWclBinary(configured);
  const args = config.get("server.args", ["lsp"]);

  const serverOptions = {
    command,
    args,
  };

  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "wcl" }],
  };

  client = new LanguageClient("wcl", "WCL Language Server", serverOptions, clientOptions);
  client.start();
}

function deactivate() {
  if (client) {
    return client.stop();
  }
}

module.exports = { activate, deactivate };
