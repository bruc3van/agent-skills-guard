import { describe, expect, it } from "vitest";
import {
  getDisplayedPluginToolIds,
  getDisplayedToolIds,
  getOperationSkillIds,
  getVisibleInstalledPaths,
  getVisiblePluginInstallPath,
  groupSkillsByName,
  normalizeInstalledSkills,
} from "./installed-skills";
import type { Plugin, Skill } from "../types";

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

function buildPlugin(overrides: Partial<Plugin>): Plugin {
  return {
    id: "plugin-1",
    name: "Plugin",
    description: undefined,
    repository_url: "local",
    repository_owner: "local",
    marketplace_name: "local",
    source: "external",
    installed: true,
    installed_at: undefined,
    claude_install_path: undefined,
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

describe("groupSkillsByName", () => {
  it("keeps same-name skills from different repositories as separate cards", () => {
    const skills = groupSkillsByName([
      buildSkill({ id: "repo-a::skill", local_path: "/tmp/a" }),
      buildSkill({
        id: "repo-b::skill",
        repository_url: "https://github.com/example/another",
        file_path: "skill-b",
        local_path: "/tmp/b",
      }),
    ]);

    expect(skills).toHaveLength(2);
    expect(skills.map((skill) => skill.id)).toEqual(["repo-a::skill", "repo-b::skill"]);
  });

  it("keeps same-name local and managed skills as separate operation targets", () => {
    const skills = groupSkillsByName([
      buildSkill({ id: "repo-a::skill", local_path: "/tmp/managed" }),
      buildSkill({
        id: "local::abc",
        repository_url: "local",
        repository_owner: "local",
        is_local_only: true,
        local_path: "/tmp/local",
      }),
    ]);

    expect(skills).toHaveLength(2);
    expect(skills.map((skill) => skill.id)).toEqual(["repo-a::skill", "local::abc"]);
  });

  it("keeps same-name local skills with different ids as separate operation targets", () => {
    const skills = groupSkillsByName([
      buildSkill({
        id: "local::abc",
        repository_url: "local",
        repository_owner: "local",
        is_local_only: true,
        local_path: "/tmp/local-a",
        checksum: "checksum-a",
      }),
      buildSkill({
        id: "local::def",
        repository_url: "local",
        repository_owner: "local",
        is_local_only: true,
        local_path: "/tmp/local-b",
        checksum: "checksum-b",
      }),
    ]);

    expect(skills).toHaveLength(2);
    expect(skills.map((skill) => skill.id)).toEqual(["local::abc", "local::def"]);
  });

  it("merges duplicate local skills with the same name and checksum across tool folders", () => {
    const [skill] = groupSkillsByName([
      buildSkill({
        id: "local::agents",
        name: "frontend-design",
        repository_url: "local",
        repository_owner: "local",
        is_local_only: true,
        local_path: "C:/Users/Bruce/.agents/skills/frontend-design",
        local_paths: ["C:/Users/Bruce/.agents/skills/frontend-design"],
        checksum: "same-checksum",
      }),
      buildSkill({
        id: "local::claude",
        name: "frontend-design",
        repository_url: "local",
        repository_owner: "local",
        is_local_only: true,
        local_path: "C:/Users/Bruce/.claude/skills/frontend-design",
        local_paths: ["C:/Users/Bruce/.claude/skills/frontend-design"],
        checksum: "same-checksum",
      }),
    ]);

    expect(skill.local_paths).toEqual([
      "C:/Users/Bruce/.agents/skills/frontend-design",
      "C:/Users/Bruce/.claude/skills/frontend-design",
    ]);
    expect(getDisplayedToolIds(skill)).toEqual(["agents", "claude-code"]);
    expect(getOperationSkillIds(skill)).toEqual(["local::agents", "local::claude"]);
  });

  it("keeps Claude Code active after one duplicate local skill is synced to Codex", () => {
    const [skill] = groupSkillsByName([
      buildSkill({
        id: "local::agents",
        name: "frontend-design",
        repository_url: "local",
        repository_owner: "local",
        is_local_only: false,
        local_path: "C:/Users/Bruce/.agents/skills/frontend-design",
        local_paths: [
          "C:/Users/Bruce/.agents/skills/frontend-design",
          "C:/Users/Bruce/.codex/skills/frontend-design",
        ],
        linked_tools: ["codex"],
        checksum: "same-checksum",
      }),
      buildSkill({
        id: "local::claude",
        name: "frontend-design",
        repository_url: "local",
        repository_owner: "local",
        is_local_only: true,
        local_path: "C:/Users/Bruce/.claude/skills/frontend-design",
        local_paths: ["C:/Users/Bruce/.claude/skills/frontend-design"],
        checksum: "same-checksum",
      }),
    ]);

    expect(skill.local_paths).toEqual([
      "C:/Users/Bruce/.agents/skills/frontend-design",
      "C:/Users/Bruce/.codex/skills/frontend-design",
      "C:/Users/Bruce/.claude/skills/frontend-design",
    ]);
    expect(getDisplayedToolIds(skill)).toEqual(["agents", "codex", "claude-code"]);
  });
});

describe("getVisibleInstalledPaths", () => {
  it("hides default tool paths for local-only skills", () => {
    const skill = buildSkill({
      id: "local::skill",
      repository_url: "local",
      repository_owner: "local",
      is_local_only: true,
      local_paths: [
        "C:/Users/Bruce/.claude/skills/example",
        "C:/Users/Bruce/.codex/skills/example",
      ],
    });

    expect(getVisibleInstalledPaths(skill)).toEqual([]);
  });

  it("hides default tool paths for managed skills", () => {
    const skill = buildSkill({
      id: "repo-a::skill",
      is_local_only: false,
      local_paths: [
        "C:/Users/Bruce/.agents/skills/example",
        "C:/Users/Bruce/.codex/skills/example",
        "C:/Users/Bruce/VSCodeProject/project/.agents/skills/example",
      ],
    });

    expect(getVisibleInstalledPaths(skill)).toEqual([
      "C:/Users/Bruce/VSCodeProject/project/.agents/skills/example",
    ]);
  });

  it("hides Windows extended-length default tool paths", () => {
    const skill = buildSkill({
      id: "repo-a::skill",
      is_local_only: false,
      local_path: "\\\\?\\C:\\Users\\Bruce\\.agents\\skills\\example",
      local_paths: ["C:/Users/Bruce/.agents/skills/example"],
    });

    expect(getVisibleInstalledPaths(skill)).toEqual([]);
  });

  it("shows project-level Codex skill paths", () => {
    const skill = buildSkill({
      id: "repo-a::skill",
      is_local_only: false,
      local_paths: [
        "C:/Users/Bruce/.codex/skills/example",
        "C:/Users/Bruce/VSCodeProject/project/.codex/skills/example",
      ],
    });

    expect(getVisibleInstalledPaths(skill)).toEqual([
      "C:/Users/Bruce/VSCodeProject/project/.codex/skills/example",
    ]);
  });
});

describe("getDisplayedToolIds", () => {
  it("infers active tools from local-only skill paths", () => {
    const skill = buildSkill({
      id: "local::skill",
      repository_url: "local",
      repository_owner: "local",
      is_local_only: true,
      local_paths: [
        "C:/Users/Bruce/.claude/skills/example",
        "C:/Users/Bruce/.codex/skills/example",
      ],
      linked_tools: [],
    });

    expect(getDisplayedToolIds(skill)).toEqual(["claude-code", "codex"]);
  });

  it("uses linked tool metadata when a local skill path is outside default tool folders", () => {
    const skill = buildSkill({
      id: "local::skill",
      repository_url: "local",
      repository_owner: "local",
      is_local_only: true,
      local_paths: ["D:/ClaudeSkills/example"],
      linked_tools: ["claude-code"],
    });

    expect(getDisplayedToolIds(skill)).toEqual(["claude-code"]);
  });

  it("shows agents plus linked tools for managed skills", () => {
    const skill = buildSkill({
      id: "repo-a::skill",
      is_local_only: false,
      linked_tools: ["codex"],
    });

    expect(getDisplayedToolIds(skill)).toEqual(["agents", "codex"]);
  });
});

describe("getDisplayedPluginToolIds", () => {
  it("classifies plugins installed under Claude Code cache as Claude Code", () => {
    const plugin = buildPlugin({
      claude_install_path:
        "C:/Users/Bruce/.claude/plugins/cache/superpowers-marketplace/superpowers/4.0.3",
    });

    expect(getDisplayedPluginToolIds(plugin)).toEqual(["claude-code"]);
  });

  it("classifies Windows extended-length Claude Code plugin cache paths", () => {
    const plugin = buildPlugin({
      claude_install_path:
        "\\\\?\\C:\\Users\\Bruce\\.claude\\plugins\\cache\\superpowers-marketplace\\superpowers\\4.0.3",
    });

    expect(getDisplayedPluginToolIds(plugin)).toEqual(["claude-code"]);
  });
});

describe("getVisiblePluginInstallPath", () => {
  it("hides default Claude Code plugin cache paths", () => {
    const plugin = buildPlugin({
      claude_install_path:
        "C:/Users/Bruce/.claude/plugins/cache/superpowers-marketplace/superpowers/4.0.3",
    });

    expect(getVisiblePluginInstallPath(plugin)).toBeUndefined();
  });

  it("shows non-default plugin install paths", () => {
    const plugin = buildPlugin({
      claude_install_path:
        "C:/Users/Bruce/VSCodeProject/project/.claude/plugins/cache/local-plugin/1.0.0",
    });

    expect(getVisiblePluginInstallPath(plugin)).toBe(
      "C:/Users/Bruce/VSCodeProject/project/.claude/plugins/cache/local-plugin/1.0.0"
    );
  });
});
