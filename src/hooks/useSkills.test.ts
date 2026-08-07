import { describe, expect, it, vi } from "vitest";
import { useInstalledSkills } from "./useSkills";

const mocks = vi.hoisted(() => ({
  useQuery: vi.fn(),
}));

vi.mock("@tanstack/react-query", () => ({
  useQuery: mocks.useQuery,
  useMutation: vi.fn(),
  useQueryClient: vi.fn(),
}));

vi.mock("../lib/api", () => ({
  api: {
    getInstalledSkills: vi.fn(),
  },
}));

describe("useInstalledSkills", () => {
  // `get_installed_skills` 并非纯读：它会遍历各工具 skill 目录、比对软链、
  // 必要时回写数据库。此前配置为 staleTime: 0 + refetchOnMount: "always"，
  // 每次切换标签页都触发一轮全盘 reconcile，是切页卡顿的主要来源。
  // 需要立即反映磁盘变化的场景都会显式 invalidate / refetch 这个 key。
  it("serves tab switches from cache instead of re-running a full disk reconcile", () => {
    mocks.useQuery.mockReturnValue({});

    useInstalledSkills();

    expect(mocks.useQuery).toHaveBeenCalledWith(
      expect.objectContaining({
        staleTime: 30_000,
        refetchOnWindowFocus: false,
      })
    );

    const options = mocks.useQuery.mock.calls[0][0];
    // 挂载即重取会让缓存形同虚设
    expect(options).not.toHaveProperty("refetchOnMount");
    // 后台轮询同样会反复触发 reconcile
    expect(options).not.toHaveProperty("refetchInterval");
  });
});
