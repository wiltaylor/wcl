# WCL Sublime Syntax

Canonical syntax definition for WCL in Sublime Text's `.sublime-syntax` format.

This file serves as the single source of truth for regex-based syntax highlighting across:

- **Sublime Text** / **Syntect** / **bat** -- use `WCL.sublime-syntax` directly
- **VS Code** / **Shiki** -- generate `wcl.tmLanguage.json` with `generate-tmlanguage.py`
- **mdbook** (syntect backend) -- use `WCL.sublime-syntax` directly

## Generating tmLanguage.json

```bash
python3 generate-tmlanguage.py > ../../editors/vscode/syntaxes/wcl.tmLanguage.json
```

Or use the just recipe:

```bash
just build vscode-syntax
```

## Installation for Sublime Text

Copy `WCL.sublime-syntax` to your Sublime Text packages directory:

```bash
cp WCL.sublime-syntax ~/.config/sublime-text/Packages/User/
```

## Installation for bat

```bash
mkdir -p "$(bat --config-dir)/syntaxes"
cp WCL.sublime-syntax "$(bat --config-dir)/syntaxes/"
bat cache --build
```
