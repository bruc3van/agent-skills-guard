// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { REPOSITORIES_PAGE_STATUS_KEY } from "../hooks/useNavigationProtection";
import { Sidebar } from "./Sidebar";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) => {
      const labels: Record<string, string> = {
        "nav.overview": "Overview",
        "nav.marketplace": "Marketplace",
        "nav.installed": "Installed",
        "nav.repositories": "Repositories",
        "nav.localCli": "CLI",
        "nav.settings": "Settings",
        "sidebar.busy": "Operation in progress",
      };
      return labels[key] ?? key;
    },
  }),
}));

function renderSidebar(queryClient: QueryClient, onTabChange = vi.fn()) {
  render(
    <QueryClientProvider client={queryClient}>
      <Sidebar currentTab="overview" onTabChange={onTabChange} />
    </QueryClientProvider>
  );
  return onTabChange;
}

afterEach(() => {
  cleanup();
});

describe("Sidebar", () => {
  it("subscribes to busy status and blocks tab changes while an operation is active", async () => {
    const user = userEvent.setup();
    const queryClient = new QueryClient();
    const onTabChange = renderSidebar(queryClient);

    await user.click(screen.getByRole("button", { name: /marketplace/i }));
    expect(onTabChange).toHaveBeenCalledWith("marketplace");

    onTabChange.mockClear();
    act(() => {
      queryClient.setQueryData(REPOSITORIES_PAGE_STATUS_KEY, {
        scanningRepoId: null,
        refreshingRepoId: null,
        deletingRepoId: null,
        preparingSkillId: "skill-a",
        installingSkillId: null,
        pendingSkillInstall: null,
      });
    });

    const marketplaceButton = screen.getByRole("button", { name: /marketplace/i });
    await waitFor(() => expect((marketplaceButton as HTMLButtonElement).disabled).toBe(true));
    expect(screen.getByText("Operation in progress")).toBeTruthy();

    await user.click(marketplaceButton);
    await user.click(screen.getByRole("button", { name: /settings/i }));
    expect(onTabChange).not.toHaveBeenCalled();

    act(() => {
      queryClient.setQueryData(REPOSITORIES_PAGE_STATUS_KEY, {
        scanningRepoId: null,
        refreshingRepoId: null,
        deletingRepoId: null,
        preparingSkillId: null,
        installingSkillId: null,
        pendingSkillInstall: null,
      });
    });

    await waitFor(() => expect((marketplaceButton as HTMLButtonElement).disabled).toBe(false));
    await user.click(marketplaceButton);
    expect(onTabChange).toHaveBeenCalledWith("marketplace");
  });
});
