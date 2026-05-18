import type { QueryClient, QueryKey } from "@tanstack/react-query";
import { AGENT_TOOLS_KEY } from "./agent-tools";

const SKILL_STATE_QUERY_KEYS: QueryKey[] = [
  ["skills"],
  ["skills", "installed"],
  ["scanResults"],
  AGENT_TOOLS_KEY,
];

export async function refetchSkillStateAfterAppUpdate(queryClient: QueryClient): Promise<void> {
  await Promise.all(
    SKILL_STATE_QUERY_KEYS.map((queryKey) =>
      queryClient.refetchQueries({ queryKey, exact: true, type: "all" })
    )
  );
}
