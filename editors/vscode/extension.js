const vscode = require("vscode");
const path = require("path");
const os = require("os");
const fs = require("fs");
const net = require("net");
const http = require("http");
const { spawn } = require("child_process");
const { LanguageClient } = require("vscode-languageclient/node");

let client;
const previews = new Map();

function findWclBinary(configured) {
  // 1. Check for bundled binary (platform-specific VSIX)
  const binName = process.platform === "win32" ? "wcl.exe" : "wcl";
  const bundled = path.join(__dirname, "bin", binName);
  if (fs.existsSync(bundled)) {
    return bundled;
  }
  // 2. User-configured absolute path
  if (path.isAbsolute(configured) && fs.existsSync(configured)) {
    return configured;
  }
  // 3. Cargo bin fallback
  const cargoBin = path.join(os.homedir(), ".cargo", "bin", "wcl");
  if (fs.existsSync(cargoBin)) {
    return cargoBin;
  }
  return configured;
}

function getWclCommand() {
  const config = vscode.workspace.getConfiguration("wcl");
  const configured = config.get("server.path", "wcl");
  return findWclBinary(configured);
}

function getFreePort() {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      const port = address && typeof address === "object" ? address.port : undefined;
      server.close(() => {
        if (port) {
          resolve(port);
        } else {
          reject(new Error("failed to allocate a free port"));
        }
      });
    });
  });
}

function waitForServer(port, child) {
  const url = `http://127.0.0.1:${port}/`;
  const deadline = Date.now() + 30000;

  return new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("close", (code, signal) => {
      const reason = signal ? `signal ${signal}` : `exit code ${code}`;
      reject(new Error(`preview server exited before it was ready (${reason})`));
    });

    const tryRequest = () => {
      if (child.exitCode !== null || child.killed) {
        reject(new Error("preview server exited before it was ready"));
        return;
      }

      const req = http.get(url, (res) => {
        res.resume();
        resolve();
      });

      req.on("error", (err) => {
        if (Date.now() >= deadline) {
          reject(err);
          return;
        }
        setTimeout(tryRequest, 200);
      });

      req.setTimeout(1000, () => {
        req.destroy();
      });
    };

    tryRequest();
  });
}

function htmlEscape(value) {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function loadingHtml(filePath) {
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <style>
    body {
      align-items: center;
      background: var(--vscode-editor-background);
      color: var(--vscode-editor-foreground);
      display: flex;
      font: 13px var(--vscode-font-family);
      height: 100vh;
      justify-content: center;
      margin: 0;
    }
  </style>
</head>
<body>Starting WCL preview for ${htmlEscape(path.basename(filePath))}...</body>
</html>`;
}

function previewHtml(url) {
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <style>
    html, body, iframe {
      border: 0;
      height: 100%;
      margin: 0;
      padding: 0;
      width: 100%;
    }
    body {
      overflow: hidden;
    }
  </style>
</head>
<body>
  <iframe src="${htmlEscape(url)}" title="WCL Preview"></iframe>
</body>
</html>`;
}

function errorHtml(message, details) {
  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <style>
    body {
      background: var(--vscode-editor-background);
      color: var(--vscode-editor-foreground);
      font: 13px var(--vscode-font-family);
      margin: 24px;
    }
    h1 {
      font-size: 16px;
      font-weight: 600;
    }
    pre {
      background: var(--vscode-textCodeBlock-background);
      overflow: auto;
      padding: 12px;
      white-space: pre-wrap;
    }
  </style>
</head>
<body>
  <h1>${htmlEscape(message)}</h1>
  ${details ? `<pre>${htmlEscape(details)}</pre>` : ""}
</body>
</html>`;
}

async function previewWcl(resource) {
  const uri = resource || (vscode.window.activeTextEditor && vscode.window.activeTextEditor.document.uri);
  if (!uri || uri.scheme !== "file" || path.extname(uri.fsPath).toLowerCase() !== ".wcl") {
    vscode.window.showErrorMessage("Select a .wcl file to preview.");
    return;
  }

  const filePath = path.normalize(uri.fsPath);
  if (!fs.existsSync(filePath)) {
    vscode.window.showErrorMessage(`WCL preview file does not exist: ${filePath}`);
    return;
  }

  const existing = previews.get(filePath);
  if (existing) {
    existing.panel.reveal(vscode.ViewColumn.Beside);
    return;
  }

  let port;
  try {
    port = await getFreePort();
  } catch (err) {
    vscode.window.showErrorMessage(`Failed to allocate a preview port: ${err.message}`);
    return;
  }
  const command = getWclCommand();
  const args = ["wdoc", "serve", filePath, "--port", String(port)];
  const title = `WCL Preview: ${path.basename(filePath)}`;
  const panel = vscode.window.createWebviewPanel(
    "wclPreview",
    title,
    vscode.ViewColumn.Beside,
    {
      enableScripts: true,
      retainContextWhenHidden: true,
      portMapping: [{ webviewPort: port, extensionHostPort: port }],
    }
  );

  panel.webview.html = loadingHtml(filePath);

  const child = spawn(command, args, {
    cwd: path.dirname(filePath),
    shell: false,
  });

  const preview = { panel, child, port, disposed: false, failed: false, ready: false, output: "" };
  previews.set(filePath, preview);

  const appendOutput = (chunk) => {
    preview.output = `${preview.output}${chunk.toString()}`;
    if (preview.output.length > 8000) {
      preview.output = preview.output.slice(-8000);
    }
  };

  child.stdout.on("data", appendOutput);
  child.stderr.on("data", appendOutput);

  child.on("error", (err) => {
    preview.failed = true;
    previews.delete(filePath);
    const message = `Failed to start WCL preview: ${err.message}`;
    vscode.window.showErrorMessage(message);
    panel.webview.html = errorHtml("Failed to start WCL preview", err.message);
  });

  child.on("close", (code, signal) => {
    if (preview.disposed || preview.failed) {
      return;
    }
    previews.delete(filePath);
    const reason = signal ? `signal ${signal}` : `exit code ${code}`;
    const title = preview.ready ? "WCL preview server stopped" : "WCL preview failed";
    panel.webview.html = errorHtml(title, `${reason}\n\n${preview.output}`.trim());
    vscode.window.showErrorMessage(`${title} (${reason}).`);
  });

  panel.onDidDispose(() => {
    preview.disposed = true;
    previews.delete(filePath);
    if (child.exitCode === null && !child.killed) {
      child.kill();
    }
  });

  try {
    await waitForServer(port, child);
    if (!preview.disposed) {
      preview.ready = true;
      panel.webview.html = previewHtml(`http://127.0.0.1:${port}/`);
    }
  } catch (err) {
    if (!preview.disposed && !preview.failed) {
      preview.failed = true;
      previews.delete(filePath);
      if (child.exitCode === null && !child.killed) {
        child.kill();
      }
      const details = `${err.message}\n\n${preview.output}`.trim();
      panel.webview.html = errorHtml("WCL preview failed", details);
      vscode.window.showErrorMessage(`WCL preview failed: ${err.message}`);
    }
  }
}

function activate(context) {
  const command = getWclCommand();
  const config = vscode.workspace.getConfiguration("wcl");
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

  context.subscriptions.push(vscode.commands.registerCommand("wcl.preview", previewWcl));
}

function deactivate() {
  for (const preview of previews.values()) {
    preview.disposed = true;
    if (preview.child.exitCode === null && !preview.child.killed) {
      preview.child.kill();
    }
    preview.panel.dispose();
  }
  previews.clear();

  if (client) {
    return client.stop();
  }
  return undefined;
}

module.exports = { activate, deactivate };
