---
name: bruce-doc-converter
description: 双向文档转换工具，将 Word (.docx)、Excel (.xlsx)、PowerPoint (.pptx) 和 PDF (.pdf) 转换为 AI 友好的 Markdown，或将 Markdown (.md) 转换为 Word (.docx)（支持 Mermaid 图表自动渲染为 PNG）。当用户请求文档转换、导出、读取、分析 Office/PDF/Markdown 文件，或上传这些格式并询问内容时使用。
---
# Bruce Doc Converter

Agent-facing document converter CLI..

## When to use

Use this skill when the user asks to:

- Convert `.docx`, `.xlsx`, `.pptx`, `.pdf`, or `.md` files.
- Read, summarize, inspect, or analyze Office/PDF documents.
- Export Markdown as Word.
- Process uploaded document files whose content the agent cannot directly read.

## Setup and version check

Run this first — it checks installation status and current version in one step:

```bash
bdc --help-json
```

- **Command not found** → not installed. Install using one of the methods below, then re-run.
- **Returns JSON with `cli_version`** → installed. Compare against the latest on PyPI:
  ```bash
  pip index versions bruce-doc-converter 2>/dev/null | head -1
  ```
  If the versions differ, upgrade (see Upgrade section), then re-run `bdc setup-node` if Markdown to Word was previously set up.

### Install (if not already installed)

Try the following in order — stop at the first that succeeds:

```bash
# 1. pipx (preferred — isolated, bdc lands in PATH)
pipx install bruce-doc-converter

# 2. uv (if available — fast, isolated, bdc lands in PATH)
uv tool install bruce-doc-converter

# 3. pip --user (most universally available, bdc lands in PATH)
pip3 install --user bruce-doc-converter  # macOS/Linux
pip install --user bruce-doc-converter   # Windows
# or universally: python3 -m pip install --user bruce-doc-converter (use `python` on Windows)

# 4. venv fallback (works everywhere, but bdc will NOT be in PATH)
python3 -m venv .venv
.venv/bin/pip install bruce-doc-converter
# Windows: .venv\Scripts\pip install bruce-doc-converter
```

> **venv note:** If you used the venv fallback, replace every `bdc` command below with `.venv/bin/bdc` (macOS/Linux) or `.venv\Scripts\bdc` (Windows).

> **Windows note:** Use `python` instead of `python3` if the former is not recognized.

## Command

Run:

```bash
bdc convert "<file>"
```

For Markdown files with Mermaid diagrams, PNG rendering defaults to scale `4`. Increase or decrease it when needed:

```bash
bdc convert "<file.md>" --mermaid-scale 5
```

For batch conversion:

```bash
bdc batch "<directory>"
bdc batch "<directory>" --mermaid-scale 5
```

For Markdown to Word, initialize the Node.js dependencies explicitly before first use:

```bash
bdc setup-node
```

If the Markdown contains **Mermaid diagrams** (` ```mermaid ` blocks), they will be automatically rendered as PNG images embedded in the Word document. The CLI automatically detects and uses the user's local Chrome / Edge / Chromium during conversion, launching it headlessly with a temporary browser profile, so do not install a bundled browser unless local browser detection fails.

If no local browser is available, explicitly install Puppeteer's dedicated browser:

```bash
bdc setup-node --install-browser
```

If your environment specifically requires npm lifecycle scripts, run:

```bash
bdc setup-node --allow-scripts --install-browser
```

> **Linux note:** Mermaid rendering is disabled by default on Linux sandboxed environments. Set `BRUCE_DOC_CONVERTER_ALLOW_CHROMIUM_NO_SANDBOX=1` if your environment requires it and you understand the risk.

The CLI prints JSON to stdout by default. Progress logs may appear on stderr.

## Output handling

Parse stdout as JSON.

On success:

- `success` is `true`.
- `output_path` points to the generated file.
- Office/PDF inputs include `markdown_content` for direct analysis.
- `.md` inputs produce a `.docx` file and may omit `markdown_content`.

On failure:

- `success` is `false`.
- Use `error_code`, `retryable`, optional `next_command`, `error`, and optional `suggestion` to decide the next step.
- Do not pre-check Python dependencies. Run the command first and react to JSON failure.
- If Markdown to Word returns `DEPENDENCY_INSTALL_REQUIRED`, run `next_command` when present, otherwise run `bdc setup-node`, then retry.
- `bdc setup-node` is idempotent and may return `already_installed: true` with `install_action: "skipped"`.

## Upgrade

To upgrade to the latest version, use the same tool you used to install:

```bash
pipx upgrade bruce-doc-converter          # if installed via pipx
uv tool upgrade bruce-doc-converter       # if installed via uv
pip install --user --upgrade bruce-doc-converter   # if installed via pip --user
.venv/bin/pip install --upgrade bruce-doc-converter  # if installed in a venv (macOS/Linux)
.venv\Scripts\pip install --upgrade bruce-doc-converter  # venv on Windows
```

After upgrading, re-run `bdc setup-node` if you use Markdown to Word conversion, as Node.js dependencies may also have been updated.

## Troubleshooting installation

| Error                                         | Cause                                         | Fix                                                                                                          |
| --------------------------------------------- | --------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| `SOCKS support` / proxy connection error    | `all_proxy` or `http_proxy` env vars set  | Run `unset all_proxy http_proxy https_proxy` (macOS/Linux) or `set all_proxy=` (Windows CMD), then retry |
| `command not found: pipx`                   | pipx not installed                            | Try `uv tool install` or `pip install --user` instead                                                    |
| `externally-managed-environment`            | Python 3.11+ system Python forbids global pip | Use `pipx`, `uv tool install`, or the venv fallback                                                      |
| Permission denied                             | No write access to install location           | Add `--user` flag, or use venv fallback                                                                    |
| `bdc: command not found` after venv install | venv bin not in PATH                          | Use full path:`.venv/bin/bdc` (macOS/Linux) or `.venv\Scripts\bdc` (Windows)                             |

## Supported formats

| Input     | Output         |
| --------- | -------------- |
| `.docx` | Markdown       |
| `.xlsx` | Markdown       |
| `.pptx` | Markdown       |
| `.pdf`  | Markdown       |
| `.md`   | Word `.docx` |
