import type { Plugin, Skill } from "../types";

type GroupedSkill = Skill & { grouped_skill_ids?: string[] };

const DEFAULT_TOOL_PATH_PATTERNS: Array<{ id: string; pattern: RegExp }> = [
  {
    id: "agents",
    pattern:
      /(?:^~|^[A-Za-z]:\/Users\/[^/]+|^\/Users\/[^/]+|^\/home\/[^/]+|^\/root)\/\.agents\/skills(?:\/|$)/,
  },
  {
    id: "claude-code",
    pattern:
      /(?:^~|^[A-Za-z]:\/Users\/[^/]+|^\/Users\/[^/]+|^\/home\/[^/]+|^\/root)\/\.claude\/skills(?:\/|$)/,
  },
  {
    id: "codex",
    pattern:
      /(?:^~|^[A-Za-z]:\/Users\/[^/]+|^\/Users\/[^/]+|^\/home\/[^/]+|^\/root)\/\.codex\/skills(?:\/|$)/,
  },
  {
    id: "antigravity",
    pattern:
      /(?:^~|^[A-Za-z]:\/Users\/[^/]+|^\/Users\/[^/]+|^\/home\/[^/]+|^\/root)\/\.gemini\/antigravity\/skills(?:\/|$)/,
  },
  {
    id: "opencode",
    pattern:
      /(?:^~|^[A-Za-z]:\/Users\/[^/]+|^\/Users\/[^/]+|^\/home\/[^/]+|^\/root)\/\.config\/opencode\/skills(?:\/|$)/,
  },
];

const DEFAULT_PLUGIN_PATH_PATTERNS: Array<{ id: string; pattern: RegExp }> = [
  {
    id: "claude-code",
    pattern:
      /(?:^~|^[A-Za-z]:\/Users\/[^/]+|^\/Users\/[^/]+|^\/home\/[^/]+|^\/root)\/\.claude\/plugins\/cache(?:\/|$)/,
  },
];

function normalizePath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  if (normalized.startsWith("//?/UNC/")) {
    return `//${normalized.slice("//?/UNC/".length)}`;
  }
  if (/^\/\/\?\/[A-Za-z]:\//.test(normalized)) {
    return normalized.slice(4);
  }
  return normalized;
}

function uniqueValues(values: string[]): string[] {
  return Array.from(new Set(values));
}

function pathsForSkill(skill: Pick<Skill, "local_path" | "local_paths" | "source_path">): string[] {
  return uniqueValues([
    ...(skill.local_paths ?? []),
    ...(skill.local_path ? [skill.local_path] : []),
    ...(skill.source_path ? [skill.source_path] : []),
  ]);
}

export function getDefaultToolIdForPath(path: string): string | null {
  const normalized = normalizePath(path);
  return DEFAULT_TOOL_PATH_PATTERNS.find(({ pattern }) => pattern.test(normalized))?.id ?? null;
}

export function isDefaultToolSkillPath(path: string): boolean {
  return getDefaultToolIdForPath(path) !== null;
}

export function normalizeInstalledSkills(skills: Skill[]): Skill[] {
  return skills.map((skill) => {
    const localPaths =
      skill.local_paths && skill.local_paths.length > 0
        ? Array.from(new Set(skill.local_paths))
        : skill.local_path
          ? [skill.local_path]
          : [];

    return {
      ...skill,
      local_paths: localPaths.length > 0 ? localPaths : undefined,
      local_path: skill.local_path ?? localPaths[0],
    };
  });
}

const nonEmpty = (v?: string | null) => (v && v !== "local" ? v : undefined);

export function getSkillGroupKey(skill: Skill): string {
  if (skill.repository_url === "local" && skill.checksum) {
    return `local::${skill.name.trim().toLowerCase()}::${skill.checksum}`;
  }

  return `id::${skill.id}`;
}

function mergeSingleGroup(group: Skill[]): GroupedSkill {
  if (group.length === 1) return group[0];

  const base = group[0];
  const linkedSet = new Set<string>();
  for (const s of group) {
    for (const tool of s.linked_tools ?? []) linkedSet.add(tool);
  }
  const paths = uniqueValues(group.flatMap(pathsForSkill));

  return {
    ...base,
    linked_tools: Array.from(linkedSet),
    local_paths: paths.length > 0 ? paths : undefined,
    local_path: base.local_path ?? paths[0],
    is_local_only: group.every((s) => s.is_local_only),
    description: group.map((s) => s.description).find(nonEmpty) ?? base.description,
    repository_url: group.map((s) => s.repository_url).find(nonEmpty) ?? base.repository_url,
    version: group.map((s) => s.version).find(nonEmpty) ?? base.version,
    author: group.map((s) => s.author).find(nonEmpty) ?? base.author,
    grouped_skill_ids: uniqueValues(group.map((s) => s.id)),
  };
}

/**
 * Only dedupe duplicate returns for the same DB record.
 * Different records must stay separate because card actions operate by skill.id.
 */
export function groupSkillsByName(skills: Skill[]): GroupedSkill[] {
  const idGroups = new Map<string, Skill[]>();
  for (const skill of skills) {
    const key = getSkillGroupKey(skill);
    const arr = idGroups.get(key);
    if (arr) arr.push(skill);
    else idGroups.set(key, [skill]);
  }
  return Array.from(idGroups.values()).map(mergeSingleGroup);
}

export function getVisibleInstalledPaths(skill: Skill): string[] {
  const paths = pathsForSkill(skill);
  return paths.filter((path) => !isDefaultToolSkillPath(path));
}

export function getDisplayedToolIds(skill: Skill): string[] {
  const pathToolIds = pathsForSkill(skill)
    .map(getDefaultToolIdForPath)
    .filter((id): id is string => Boolean(id));

  if (skill.is_local_only) {
    return uniqueValues([...(skill.linked_tools ?? []), ...pathToolIds]);
  }

  return uniqueValues(["agents", ...(skill.linked_tools ?? []), ...pathToolIds]);
}

export function getOperationSkillIds(skill: GroupedSkill): string[] {
  return skill.grouped_skill_ids && skill.grouped_skill_ids.length > 0
    ? skill.grouped_skill_ids
    : [skill.id];
}

export function getDefaultToolIdForPluginPath(path: string): string | null {
  const normalized = normalizePath(path);
  return DEFAULT_PLUGIN_PATH_PATTERNS.find(({ pattern }) => pattern.test(normalized))?.id ?? null;
}

export function getVisiblePluginInstallPath(
  plugin: Pick<Plugin, "claude_install_path">
): string | undefined {
  const installPath = plugin.claude_install_path?.trim();
  if (!installPath || getDefaultToolIdForPluginPath(installPath)) return undefined;
  return installPath;
}

export function getDisplayedPluginToolIds(plugin: Pick<Plugin, "claude_install_path">): string[] {
  const installPath = plugin.claude_install_path?.trim();
  if (!installPath) return [];

  return uniqueValues(
    [getDefaultToolIdForPluginPath(installPath)].filter((id): id is string => Boolean(id))
  );
}
