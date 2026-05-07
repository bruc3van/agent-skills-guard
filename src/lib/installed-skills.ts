import type { Skill } from "../types";

const DEFAULT_TOOL_PATH_PATTERNS: Array<{ id: string; pattern: RegExp }> = [
  { id: "agents", pattern: /(?:^~|^[A-Za-z]:\/Users\/[^/]+|^\/Users\/[^/]+|^\/home\/[^/]+|^\/root)\/\.agents\/skills(?:\/|$)/ },
  { id: "claude-code", pattern: /(?:^~|^[A-Za-z]:\/Users\/[^/]+|^\/Users\/[^/]+|^\/home\/[^/]+|^\/root)\/\.claude\/skills(?:\/|$)/ },
  { id: "codex", pattern: /(?:^~|^[A-Za-z]:\/Users\/[^/]+|^\/Users\/[^/]+|^\/home\/[^/]+|^\/root)\/\.codex\/skills(?:\/|$)/ },
  { id: "antigravity", pattern: /(?:^~|^[A-Za-z]:\/Users\/[^/]+|^\/Users\/[^/]+|^\/home\/[^/]+|^\/root)\/\.gemini\/antigravity\/skills(?:\/|$)/ },
  { id: "opencode", pattern: /(?:^~|^[A-Za-z]:\/Users\/[^/]+|^\/Users\/[^/]+|^\/home\/[^/]+|^\/root)\/\.config\/opencode\/skills(?:\/|$)/ },
];

function normalizePath(path: string): string {
  return path.replace(/\\/g, "/");
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

function mergeSingleGroup(group: Skill[]): Skill {
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
  };
}

function isLocalSkill(skill: Skill): boolean {
  return skill.is_local_only === true || !skill.repository_url || skill.repository_url === "local";
}

/**
 * 两阶段合并：
 *   Pass 1 — 按 skill.id 合并同一 DB 记录的重复返回项
 *   Pass 2 — 按 name 合并：
 *     - local + local（同名，不同工具目录）→ 合并为一张卡片，两个按钮均点亮
 *     - local + managed（同名）          → 合并到 managed，保留 GitHub URL
 *     - managed + managed（同名，不同仓库）→ 保持独立（不同项目的同名 skill）
 */
export function groupSkillsByName(skills: Skill[]): Skill[] {
  // Pass 1: 按 id 去重同一 DB 记录
  const idGroups = new Map<string, Skill[]>();
  for (const skill of skills) {
    const arr = idGroups.get(skill.id);
    if (arr) arr.push(skill);
    else idGroups.set(skill.id, [skill]);
  }
  const pass1 = Array.from(idGroups.values()).map(mergeSingleGroup);

  // Pass 2: 按规范化 name 分组，合并 local 与 managed
  const byName = new Map<string, { managed: Skill[]; local: Skill[] }>();
  for (const skill of pass1) {
    const name = skill.name.toLowerCase();
    const entry = byName.get(name) ?? { managed: [], local: [] };
    if (isLocalSkill(skill)) entry.local.push(skill);
    else entry.managed.push(skill);
    byName.set(name, entry);
  }

  const result: Skill[] = [];
  for (const { managed, local } of byName.values()) {
    if (local.length === 0) {
      // 无 local —— 不同仓库的同名 managed 各自保持独立
      result.push(...managed);
    } else if (managed.length === 0) {
      // 纯 local —— 合并（同一 skill 散落在多个工具目录）
      result.push(mergeSingleGroup(local));
    } else {
      // local + managed：将所有 local 合并进第一个 managed；其余 managed 各自保持独立
      result.push(mergeSingleGroup([managed[0], ...local]));
      result.push(...managed.slice(1));
    }
  }

  return result;
}

export function getVisibleInstalledPaths(skill: Skill): string[] {
  const paths = pathsForSkill(skill);
  return paths.filter((path) => !isDefaultToolSkillPath(path));
}

export function getDisplayedToolIds(skill: Skill): string[] {
  if (skill.is_local_only) {
    return uniqueValues(
      pathsForSkill(skill)
        .map(getDefaultToolIdForPath)
        .filter((id): id is string => Boolean(id))
    );
  }

  return uniqueValues(["agents", ...(skill.linked_tools ?? [])]);
}
