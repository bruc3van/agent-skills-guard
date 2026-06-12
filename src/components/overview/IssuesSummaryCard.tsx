import React from "react";
import { useTranslation } from "react-i18next";
import { AlertTriangle, AlertCircle, Shield } from "lucide-react";

interface IssuesSummaryCardProps {
  issuesByLevel: Record<string, number>;
  filterLevel: string | null;
  onFilterChange: (level: string | null) => void;
}

const levelConfig = {
  Severe: {
    icon: AlertTriangle,
    iconBg: "bg-red-500",
    textColor: "text-red-600",
    selectedBg: "bg-red-50",
    selectedRing: "ring-red-200",
  },
  MidHigh: {
    icon: AlertCircle,
    iconBg: "bg-orange-500",
    textColor: "text-orange-600",
    selectedBg: "bg-orange-50",
    selectedRing: "ring-orange-200",
  },
  Safe: {
    icon: Shield,
    iconBg: "bg-green-500",
    textColor: "text-green-600",
    selectedBg: "bg-green-50",
    selectedRing: "ring-green-200",
  },
};

export const IssuesSummaryCard = React.memo(function IssuesSummaryCard({
  issuesByLevel,
  filterLevel,
  onFilterChange,
}: IssuesSummaryCardProps) {
  const { t } = useTranslation();

  return (
    <div className="grid grid-cols-3 grid-rows-1 gap-4 h-full">
      {Object.entries(levelConfig).map(([level, config]) => {
        const Icon = config.icon;
        const count = issuesByLevel[level] || 0;
        const isSelected = filterLevel === level;

        return (
          <button
            key={level}
            onClick={() => onFilterChange(isSelected ? null : level)}
            className={`apple-card p-5 text-left transition-all duration-200 h-full ${
              isSelected
                ? `${config.selectedBg} ring-2 ${config.selectedRing}`
                : "hover:scale-[1.02]"
            }`}
          >
            <div
              className={`w-9 h-9 rounded-xl ${config.iconBg} flex items-center justify-center mb-3 shadow-lg shadow-black/10`}
            >
              <Icon className="w-4 h-4 text-white" strokeWidth={2.5} />
            </div>
            <div className={`text-3xl font-semibold tracking-tight ${config.textColor}`}>
              {count}
            </div>
            <div className="text-xs text-muted-foreground mt-1 font-medium">
              {t(`overview.riskFilters.${level}`)}
            </div>
          </button>
        );
      })}
    </div>
  );
});
