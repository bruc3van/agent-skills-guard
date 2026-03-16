import type { Skill } from "../types";

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
      local_path: skill.local_path ?? localPaths.at(-1),
    };
  });
}
