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

export function managerLabel(manager: string): string {
  const labels: Record<string, string> = {
    npm: "npm",
    pnpm: "pnpm",
    pip: "pip",
    brew: "Homebrew",
    scoop: "Scoop",
    choco: "Chocolatey",
    unknown: "未知来源",
  };
  return labels[manager] ?? manager;
}

export function canAutoUpdate(tool: LocalCliTool): boolean {
  return tool.manager !== "unknown" && !!tool.package_name;
}
