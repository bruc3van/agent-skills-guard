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
  it("refreshes when the installed page mounts without polling in the background", () => {
    mocks.useQuery.mockReturnValue({});

    useInstalledSkills();

    expect(mocks.useQuery).toHaveBeenCalledWith(
      expect.objectContaining({
        staleTime: 0,
        refetchOnMount: "always",
        refetchOnWindowFocus: false,
      })
    );
    expect(mocks.useQuery.mock.calls[0][0]).not.toHaveProperty("refetchInterval");
  });
});
