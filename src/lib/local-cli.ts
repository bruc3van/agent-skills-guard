import type { LocalCliTool } from "../types";

export function groupByManager(tools: LocalCliTool[]): Record<string, LocalCliTool[]> {
  return tools.reduce(
    (acc, t) => {
      if (!acc[t.manager]) acc[t.manager] = [];
      acc[t.manager].push(t);
      return acc;
    },
    {} as Record<string, LocalCliTool[]>
  );
}

export function managerLabel(manager: string, translate?: (key: string) => string): string {
  const labels: Record<string, string> = {
    npm: "npm",
    pnpm: "pnpm",
    pip: "pip",
    brew: "Homebrew",
    scoop: "Scoop",
    choco: "Chocolatey",
  };
  if ((manager === "native" || manager === "unknown") && translate) {
    return translate(`localCli.managers.${manager}`);
  }
  if (manager === "native") return "Native";
  if (manager === "unknown") return "Unknown";
  return labels[manager] ?? manager;
}

export function canNativeSelfUpdate(tool: LocalCliTool): boolean {
  return tool.manager === "native" && ["grok", "claude", "codex"].includes(tool.id);
}

export function canAutoUpdate(tool: LocalCliTool): boolean {
  return tool.manager !== "unknown" && !!tool.package_name;
}
