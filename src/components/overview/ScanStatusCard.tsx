import { useTranslation } from "react-i18next";
import { CheckCircle, Activity, Clock } from "lucide-react";
import { formatDistanceToNow } from "date-fns";
import { zhCN, enUS } from "date-fns/locale";

interface ScanStatusCardProps {
  lastScanTime: Date | null;
  scannedCount: number;
  totalCount: number;
  issueCount: number;
  isScanning: boolean;
  countLabel: string;
  scanLabel: string;
  fileProgress?: { scanned: number; total: number } | null;
}

export function ScanStatusCard({
  lastScanTime,
  scannedCount,
  totalCount,
  issueCount,
  isScanning,
  countLabel,
  scanLabel,
  fileProgress,
}: ScanStatusCardProps) {
  const { t, i18n } = useTranslation();

  const rawProgress = totalCount > 0 ? Math.min(100, (scannedCount / totalCount) * 100) : 0;
  const progress = isScanning ? Math.min(rawProgress, 99) : rawProgress;
  const isComplete = totalCount > 0 && scannedCount >= totalCount;
  const fileProgressDisplay =
    fileProgress && fileProgress.total > 0 ? fileProgress : null;
  const noIssueDisplay = fileProgressDisplay ?? { scanned: scannedCount, total: totalCount };
  const noIssueLabel = fileProgressDisplay ? t("overview.scanStatus.files") : countLabel;
  const showIndeterminate = isScanning && (totalCount === 0 || scannedCount === 0);

  const formatRelativeTime = (date: Date) => {
    const locale = i18n.language === "zh" ? zhCN : enUS;
    return formatDistanceToNow(date, { addSuffix: true, locale });
  };

  const getStatusInfo = () => {
    if (isScanning) return { color: "text-blue-500", bg: "bg-blue-500", label: "scanning" };
    if (isComplete && issueCount === 0)
      return { color: "text-green-600", bg: "bg-green-500", label: "safe" };
    if (isComplete && issueCount > 0)
      return { color: "text-orange-500", bg: "bg-orange-500", label: "warning" };
    return { color: "text-blue-500", bg: "bg-blue-500", label: "default" };
  };

  const status = getStatusInfo();
  const countClass = "text-2xl font-semibold text-foreground tabular-nums";
  const labelClass = isScanning ? "text-[11px] text-muted-foreground" : "text-xs text-muted-foreground";

  return (
    <div className="apple-card p-6 h-full">
      <div className="space-y-5">
        {/* 顶部信息区 */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 rounded-full bg-secondary flex items-center justify-center">
              <Clock className="w-5 h-5 text-muted-foreground" />
            </div>
            <div>
              <div className="text-xs text-muted-foreground mb-0.5">
                {t("overview.scanStatus.lastScan")}
              </div>
              <div className={`text-sm font-semibold ${status.color}`}>
                {lastScanTime ? formatRelativeTime(lastScanTime) : t("overview.scanStatus.never")}
              </div>
            </div>
          </div>
          <div className="text-right">
            <div className={countClass}>
              {scannedCount}
              <span className="text-muted-foreground"> / {totalCount}</span>
            </div>
            <div className={labelClass}>{countLabel}</div>
          </div>
        </div>

        {/* 进度条 */}
        <div className="space-y-2">
          <div
            className={`apple-progress ${isScanning ? "h-5 apple-progress-active" : "h-4"} ${
              showIndeterminate ? "apple-progress-indeterminate" : ""
            }`}
          >
            <div
              className={`h-full rounded-full transition-[width] duration-700 ease-out ${status.bg} ${
                isScanning ? "apple-progress-bar-active" : ""
              }`}
              style={{ width: `${progress}%` }}
            />
          </div>
          {/* 状态文字 */}
          <div className="flex items-center gap-2">
            {isScanning ? (
              <>
                <Activity className="w-4 h-4 text-blue-500 animate-pulse" />
                <span className="text-sm text-blue-500 font-medium">
                  {fileProgressDisplay
                    ? t("overview.scanStatus.scanningWithProgress", {
                        scanned: fileProgressDisplay.scanned,
                        total: fileProgressDisplay.total,
                        label: t("overview.scanStatus.files"),
                      })
                    : `${scanLabel}...`}
                </span>
              </>
            ) : isComplete ? (
              <>
                <CheckCircle
                  className={`w-4 h-4 ${issueCount === 0 ? "text-green-600" : "text-orange-500"}`}
                />
                <span
                  className={`text-sm font-medium ${issueCount === 0 ? "text-green-600" : "text-orange-500"}`}
                >
                  {issueCount === 0
                    ? t("overview.scanStatus.noIssues", {
                        scanned: noIssueDisplay.scanned,
                        total: noIssueDisplay.total,
                        label: noIssueLabel,
                      })
                    : t("overview.scanStatus.completed", { count: issueCount })}
                </span>
              </>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}
