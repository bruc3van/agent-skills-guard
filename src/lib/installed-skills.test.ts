import { describe, expect, it } from "vitest";
import { normalizeInstalledSkills } from "./installed-skills";
import type { Skill } from "../types";

function buildSkill(overrides: Partial<Skill>): Skill {
  return {
    id: "skill-1",
    name: "Duplicate Name",
    description: undefined,
    repository_url: "https://github.com/example/repo",
    repository_owner: "example",
    file_path: "skill-a",
    version: undefined,
    author: undefined,
    installed: true,
    installed_at: undefined,
    local_path: undefined,
    local_paths: undefined,
    checksum: undefined,
    security_score: undefined,
    security_issues: undefined,
    installed_commit_sha: undefined,
    ...overrides,
  };
}

describe("normalizeInstalledSkills", () => {
  it("keeps same-name skills as separate entries", () => {
    const skills = normalizeInstalledSkills([
      buildSkill({ id: "repo-a::skill", local_path: "/tmp/a" }),
      buildSkill({
        id: "repo-b::skill",
        repository_url: "https://github.com/example/another",
        file_path: "skill-b",
        local_path: "/tmp/b",
      }),
    ]);

    expect(skills).toHaveLength(2);
    expect(skills[0].id).toBe("repo-a::skill");
    expect(skills[1].id).toBe("repo-b::skill");
  });

  it("hydrates local_paths from local_path when needed", () => {
    const [skill] = normalizeInstalledSkills([
      buildSkill({
        id: "repo-a::skill",
        local_path: "/tmp/a",
        local_paths: undefined,
      }),
    ]);

    expect(skill.local_paths).toEqual(["/tmp/a"]);
    expect(skill.local_path).toBe("/tmp/a");
  });
});
