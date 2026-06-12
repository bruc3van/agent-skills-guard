import React from "react";
import { useTranslation } from "react-i18next";
import { Package, FolderGit, Plug, Store } from "lucide-react";

interface StatisticsCardsProps {
  installedCount: number;
  pluginCount: number;
  marketplaceCount: number;
  repositoryCount: number;
}

// Apple 官方色彩
const cards = [
  {
    key: "installedSkills",
    icon: Package,
    gradient: "from-blue-500 to-blue-600",
    iconBg: "bg-blue-500",
  },
  {
    key: "installedPlugins",
    icon: Plug,
    gradient: "from-indigo-500 to-indigo-600",
    iconBg: "bg-indigo-500",
  },
  {
    key: "marketplaces",
    icon: Store,
    gradient: "from-fuchsia-500 to-fuchsia-600",
    iconBg: "bg-fuchsia-500",
  },
  {
    key: "repositories",
    icon: FolderGit,
    gradient: "from-green-500 to-green-600",
    iconBg: "bg-green-500",
  },
];

export const StatisticsCards = React.memo(function StatisticsCards({
  installedCount,
  pluginCount,
  marketplaceCount,
  repositoryCount,
}: StatisticsCardsProps) {
  const { t } = useTranslation();
  const counts = [installedCount, pluginCount, marketplaceCount, repositoryCount];

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-5">
      {cards.map((card, index) => {
        const Icon = card.icon;
        const count = counts[index];

        return (
          <div key={card.key} className="apple-card p-6 group">
            <div className="flex items-start justify-between">
              <div>
                <div className="apple-stat-value text-foreground">{count}</div>
                <div className="apple-stat-label">{t(`overview.statistics.${card.key}`)}</div>
              </div>
              <div
                className={`w-11 h-11 rounded-2xl ${card.iconBg} flex items-center justify-center shadow-lg shadow-black/10 group-hover:scale-105 transition-transform duration-300`}
              >
                <Icon className="w-5 h-5 text-white" strokeWidth={2} />
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
});
