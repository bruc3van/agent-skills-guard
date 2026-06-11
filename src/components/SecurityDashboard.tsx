import { useState, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { Search, Loader2, Shield } from "lucide-react";
import { SecurityDetailDialog } from "./SecurityDetailDialog";
import { CyberSelect, type CyberSelectOption } from "./ui/CyberSelect";
import type { SkillScanResult } from "@/types/security";
import { countIssuesBySeverity } from "@/lib/security-utils";
import { appToast } from "@/lib/toast";
import { getScanConcurrency, getScanPolicy } from "@/lib/storage";

export function SecurityDashboard() {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const [isScanning, setIsScanning] = useState(false);
  const [filterLevel, setFilterLevel] = useState<string>("all");
  const [sortBy, setSortBy] = useState<"score" | "name" | "time">("score");
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedSkill, setSelectedSkill] = useState<SkillScanResult | null>(null);

  const levelOptions: CyberSelectOption[] = [
    { value: "all", label: t("security.levels.all") },
    { value: "Critical", label: t("security.levels.critical") },
    { value: "High", label: t("security.levels.high") },
    { value: "Medium", label: t("security.levels.medium") },
    { value: "Low", label: t("security.levels.low") },
    { value: "Safe", label: t("security.levels.safe") },
  ];

  const sortOptions: CyberSelectOption[] = [
    { value: "score", label: t("security.sort.score") },
    { value: "name", label: t("security.sort.name") },
    { value: "time", label: t("security.sort.time") },
  ];

  const { data: scanResults = [], isLoading } = useQuery<SkillScanResult[]>({
    queryKey: ["scanResults"],
    queryFn: async () => {
      return await invoke("get_scan_results");
    },
  });

  const handleScan = async () => {
    setIsScanning(true);
    try {
      const scanConcurrency = getScanConcurrency();
      const policy = getScanPolicy();
      const results = await invoke<SkillScanResult[]>("scan_all_installed_skills", {
        locale: i18n.language,
        scanParallelism: scanConcurrency,
        scanPolicy: policy,
      });
      queryClient.invalidateQueries({ queryKey: ["scanResults"] });
      appToast.banner(t("security.dashboard.scanSuccess", { count: results.length }), {
        tone: "success",
      });
    } catch (error) {
      console.error("Scan failed:", error);
      appToast.banner(t("security.dashboard.scanError"), { tone: "error" });
    } finally {
      setIsScanning(false);
    }
  };

  const filteredAndSortedResults = useMemo(() => {
    return scanResults
      .filter((result) => {
        if (filterLevel !== "all" && result.level !== filterLevel) {
          return false;
        }
        if (searchQuery && !result.skill_name.toLowerCase().includes(searchQuery.toLowerCase())) {
          return false;
        }
        return true;
      })
      .sort((a, b) => {
        switch (sortBy) {
          case "score":
            return a.score - b.score;
          case "name":
            return a.skill_name.localeCompare(b.skill_name);
          case "time":
            return new Date(b.scanned_at).getTime() - new Date(a.scanned_at).getTime();
          default:
            return 0;
        }
      });
  }, [scanResults, filterLevel, searchQuery, sortBy]);

  return (
    <div className="space-y-6">
      {/* 顶部操作栏 */}
      <div className="flex justify-between items-center">
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-lg bg-primary/10">
            <Shield className="w-5 h-5 text-primary" />
          </div>
          <h1 className="text-lg font-semibold text-foreground">{t("security.dashboard.title")}</h1>
        </div>
        <button
          onClick={handleScan}
          disabled={isScanning}
          className="macos-button-primary disabled:opacity-50"
        >
          {isScanning ? (
            <>
              <Loader2 className="w-4 h-4 animate-spin" />
              {t("security.dashboard.scanning")}
            </>
          ) : (
            <>
              <Shield className="w-4 h-4" />
              {t("security.dashboard.scanButton")}
            </>
          )}
        </button>
      </div>

      {/* 过滤和排序栏 */}
      <div className="flex flex-wrap items-center gap-4 p-4 bg-muted/30 rounded-lg border border-border">
        <div className="flex items-center gap-2">
          <label className="text-sm text-muted-foreground whitespace-nowrap">
            {t("security.filterByLevel")}:
          </label>
          <CyberSelect
            value={filterLevel}
            onChange={setFilterLevel}
            options={levelOptions}
            className="w-[240px]"
          />
        </div>

        <div className="flex items-center gap-2">
          <label className="text-sm text-muted-foreground whitespace-nowrap">
            {t("security.sortBy")}:
          </label>
          <CyberSelect
            value={sortBy}
            onChange={(value) => setSortBy(value as "score" | "name" | "time")}
            options={sortOptions}
            className="w-[200px]"
          />
        </div>

        <div className="flex-1 min-w-[200px]">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-muted-foreground" />
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder={t("security.search")}
              className="w-full pl-10 pr-4 py-2 bg-card border border-border rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-primary/50 transition-all"
            />
          </div>
        </div>
      </div>

      {/* Skills 列表表格 */}
      <div className="macos-card overflow-hidden">
        {isLoading ? (
          <div className="flex items-center justify-center py-12">
            <Loader2 className="w-8 h-8 animate-spin text-primary" />
          </div>
        ) : filteredAndSortedResults.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 text-muted-foreground">
            <Shield className="w-12 h-12 mb-4" />
            <p>{t("security.noResults")}</p>
          </div>
        ) : (
          <table className="w-full">
            <thead className="bg-muted/30 border-b border-border">
              <tr>
                <th className="px-6 py-3 text-left text-xs text-muted-foreground uppercase">
                  {t("security.table.skillName")}
                </th>
                <th className="px-6 py-3 text-center text-xs text-muted-foreground uppercase">
                  {t("security.table.score")}
                </th>
                <th className="px-6 py-3 text-center text-xs text-muted-foreground uppercase">
                  {t("security.table.level")}
                </th>
                <th className="px-6 py-3 text-center text-xs text-muted-foreground uppercase">
                  {t("security.table.issues")}
                </th>
                <th className="px-6 py-3 text-center text-xs text-muted-foreground uppercase">
                  {t("security.table.lastScan")}
                </th>
                <th className="px-6 py-3 text-center text-xs text-muted-foreground uppercase">
                  {t("security.table.actions")}
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border">
              {filteredAndSortedResults.map((result) => {
                const issueCounts = countIssuesBySeverity(result.report.issues);

                return (
                  <tr key={result.skill_id} className="hover:bg-muted/30 transition-colors">
                    <td className="px-6 py-4 text-sm">{result.skill_name}</td>
                    <td className="px-6 py-4 text-center">
                      <ScoreDisplay score={result.score} />
                    </td>
                    <td className="px-6 py-4 text-center">
                      <SecurityBadge level={result.level} />
                    </td>
                    <td className="px-6 py-4 text-center">
                      <div className="flex items-center justify-center gap-2 text-xs">
                        {issueCounts.critical > 0 && (
                          <span className="text-destructive">C:{issueCounts.critical}</span>
                        )}
                        {issueCounts.high > 0 && (
                          <span className="text-orange-500">H:{issueCounts.high}</span>
                        )}
                        {issueCounts.medium > 0 && (
                          <span className="text-warning">M:{issueCounts.medium}</span>
                        )}
                      </div>
                    </td>
                    <td className="px-6 py-4 text-center text-xs text-muted-foreground">
                      {new Date(result.scanned_at).toLocaleString()}
                    </td>
                    <td className="px-6 py-4 text-center">
                      <button
                        onClick={() => setSelectedSkill(result)}
                        className="px-3 py-1.5 text-xs border border-primary text-primary rounded-lg hover:bg-primary/10 transition-colors"
                      >
                        {t("security.table.viewDetails")}
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      <SecurityDetailDialog
        result={selectedSkill}
        open={selectedSkill !== null}
        onClose={() => setSelectedSkill(null)}
      />
    </div>
  );
}

function SecurityBadge({ level }: { level: string }) {
  const colors = {
    Safe: "bg-success/10 text-success border-success/30",
    Low: "bg-primary/10 text-primary border-primary/30",
    Medium: "bg-warning/10 text-warning border-warning/30",
    High: "bg-orange-500/10 text-orange-500 border-orange-500/30",
    Critical: "bg-destructive/10 text-destructive border-destructive/30",
  };

  return (
    <span
      className={`px-2 py-1 rounded-lg text-xs font-medium border ${colors[level as keyof typeof colors] || colors.Safe}`}
    >
      {level}
    </span>
  );
}

function ScoreDisplay({ score }: { score: number }) {
  const getColor = (score: number) => {
    if (score >= 90) return "text-success";
    if (score >= 70) return "text-warning";
    if (score >= 50) return "text-orange-500";
    return "text-destructive";
  };

  return <span className={`text-2xl font-bold ${getColor(score)}`}>{score}</span>;
}
