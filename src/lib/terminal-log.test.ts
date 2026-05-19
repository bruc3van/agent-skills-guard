import { describe, expect, it } from "vitest";
import { sanitizeTerminalText } from "./terminal-log";

describe("sanitizeTerminalText", () => {
  it("removes ANSI, OSC titles, and spinner frames from npm logs", () => {
    const raw =
      '\u001b[?9001h\u001b[?1004h\u001b[?25l\u001b[2J\u001b[m\u001b[H\u001b]0;npm\u0007\u001b[?25h\u001b[1mnpm\u001b[22m \u001b[33mwarn \u001b[94mUnknown env config "_jsr-registry". This will stop working in the next major version of npm.\r\n\u001b]0;npm install @openai/codex\u0007\u001b[m⠙\u001b[K\r⠹\u001b[K\r\u001b[Kadded 1 package in 2s\r\n';

    const log = sanitizeTerminalText(raw);

    expect(log).not.toContain("\u001b");
    expect(log).not.toContain("]0;");
    expect(log).not.toContain("⠙");
    expect(log).toContain(
      'npm warn Unknown env config "_jsr-registry". This will stop working in the next major version of npm.'
    );
    expect(log).toContain("added 1 package in 2s");
  });

  it("renders carriage-return progress while preserving CRLF lines", () => {
    expect(sanitizeTerminalText("first line\r\nInstalling 1%\rInstalling 100%\r\nDone\r\n")).toBe(
      "first line\nInstalling 100%\nDone"
    );
  });

  it("removes block-character banner lines", () => {
    const raw = "opencode\r\n█▀▀█ █▀▀█ █▀▀█ █▀▀▄ █▀▀▀ █▀▀█ █▀▀█ █▀▀█\r\nopencode installed\r\n";

    expect(sanitizeTerminalText(raw)).toBe("opencode\nopencode installed");
  });
});
