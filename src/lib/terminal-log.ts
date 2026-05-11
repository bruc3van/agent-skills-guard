const ANSI_RE =
  // eslint-disable-next-line no-control-regex
  /[\u001b\u009b][[\]()#;?]*(?:(?:(?:[a-zA-Z\d]*(?:;[a-zA-Z\d]*)*)?\u0007)|(?:(?:\d{1,4}(?:;\d{0,4})*)?[\dA-PR-TZcf-nq-uy=><~]))/g;

const PROGRESS_FRAME_RE = /^[\u280b\u2819\u2839\u2838\u283c\u2834\u2826\u2827\u2807\u280f\-\\|/]$/;
const ASCII_ART_CHAR_RE = /[\u2500-\u257f\u2580-\u259f\u25a0-\u25ff\u2800-\u28ff\u2b00-\u2bff\/\\|_=#@*~^+]/g;

export function sanitizeTerminalText(value: string): string {
  const stripped = value.replace(ANSI_RE, "");
  const rendered = renderTerminalLineControls(stripped);
  return rendered
    .split("\n")
    .map((line) => line.trimEnd())
    .filter((line) => line.trim().length > 0)
    .filter((line) => !PROGRESS_FRAME_RE.test(line.trim()))
    .filter((line) => !isAsciiArtLine(line.trim()))
    .join("\n")
    .trim();
}

function renderTerminalLineControls(value: string): string {
  let output = "";
  let currentLine = "";

  for (let i = 0; i < value.length; i += 1) {
    const char = value[i];
    if (char === "\r") {
      if (value[i + 1] === "\n") {
        output += `${currentLine}\n`;
        currentLine = "";
        i += 1;
      } else {
        currentLine = "";
      }
      continue;
    }

    if (char === "\n") {
      output += `${currentLine}\n`;
      currentLine = "";
      continue;
    }

    if (char === "\b") {
      currentLine = currentLine.slice(0, -1);
      continue;
    }

    if (char < " " && char !== "\t") {
      continue;
    }

    currentLine += char;
  }

  return output + currentLine;
}

function isAsciiArtLine(line: string): boolean {
  const compact = line.replace(/\s/g, "");
  if (compact.length < 8) {
    return false;
  }

  const visualCount = (compact.match(ASCII_ART_CHAR_RE) || []).length;
  const alphanumericCount = (compact.match(/[a-zA-Z0-9]/g) || []).length;

  return visualCount / compact.length >= 0.45 && alphanumericCount / compact.length <= 0.55;
}
