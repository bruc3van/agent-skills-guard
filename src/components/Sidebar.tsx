import { useTranslation } from "react-i18next";
import { LayoutDashboard, Package, ShoppingCart, Database, Settings, Terminal } from "lucide-react";
import type { TabType } from "../types";

interface SidebarProps {
  currentTab: TabType;
  onTabChange: (tab: TabType) => void;
}

const mainNavItems: { id: TabType; icon: typeof LayoutDashboard; labelKey: string }[] = [
  { id: "overview", icon: LayoutDashboard, labelKey: "nav.overview" },
  { id: "marketplace", icon: ShoppingCart, labelKey: "nav.marketplace" },
  { id: "installed", icon: Package, labelKey: "nav.installed" },
  { id: "repositories", icon: Database, labelKey: "nav.repositories" },
  { id: "local-cli", icon: Terminal, labelKey: "nav.localCli" },
];

const settingsItem = { id: "settings" as TabType, icon: Settings, labelKey: "nav.settings" };

export function Sidebar({ currentTab, onTabChange }: SidebarProps) {
  const { t } = useTranslation();

  return (
    <aside className="w-[240px] flex-shrink-0 bg-sidebar flex flex-col">
      {/* Main Navigation */}
      <nav className="p-4 space-y-1 flex-1 overflow-y-auto">
        {mainNavItems.map((item) => {
          const Icon = item.icon;
          const isActive = currentTab === item.id;

          return (
            <button
              key={item.id}
              onClick={() => onTabChange(item.id)}
              className={`
                sidebar-item w-full
                ${isActive ? "sidebar-item-active" : ""}
              `}
            >
              <Icon className="w-[18px] h-[18px]" strokeWidth={isActive ? 2.5 : 2} />
              <span>{t(item.labelKey)}</span>
            </button>
          );
        })}
      </nav>

      {/* Settings at Bottom */}
      <div className="p-4 border-t border-border/50">
        <button
          onClick={() => onTabChange(settingsItem.id)}
          className={`
            sidebar-item w-full
            ${currentTab === settingsItem.id ? "sidebar-item-active" : ""}
          `}
        >
          <settingsItem.icon
            className="w-[18px] h-[18px]"
            strokeWidth={currentTab === settingsItem.id ? 2.5 : 2}
          />
          <span>{t(settingsItem.labelKey)}</span>
        </button>
      </div>
    </aside>
  );
}
