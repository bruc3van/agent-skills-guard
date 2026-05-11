import { useState } from "react";
import { RefreshCw, Loader2, Terminal, Search, RefreshCcw } from "lucide-react";
import {
  useLocalCliTools,
  useCheckLocalCliUpdates,
  useUpdateLocalCliTool,
} from "../hooks/useLocalCli";
import { LocalCliToolRow } from "./ui/LocalCliToolRow";
import { groupByManager, managerLabel } from "../lib/local-cli";
import { useTranslation } from "react-i18next";

export function LocalCliPage() {
  const { t } = useTranslation();
  const { data: tools = [], isLoading, refetch } = useLocalCliTools();
  const { mutate: checkUpdates, isPending: isChecking } = useCheckLocalCliUpdates();
  const {
    mutate: updateTool,
    isPending: isUpdating,
    variables: updatingId,
  } = useUpdateLocalCliTool();
  const [search, setSearch] = useState("");

  const filtered = tools.filter(
    (tool) =>
      tool.id.includes(search.toLowerCase()) ||
      tool.detected_path.toLowerCase().includes(search.toLowerCase())
  );
  const groups = groupByManager(filtered);
  const updateCount = tools.filter((tool) => tool.update_available).length;

  const handleCheckUpdates = () => {
    checkUpdates(undefined, { onSuccess: () => void refetch() });
  };

  return (
    <div className="h-full overflow-y-auto">
      <div className="p-8 animate-fade-in">
        <div className="max-w-5xl mx-auto space-y-5">
          <div className="flex items-start justify-between gap-4">
            <div>
              <div className="flex items-center gap-2 mb-1">
                <Terminal className="w-5 h-5 text-primary" />
                <h1 className="text-xl font-semibold tracking-tight">{t("localCli.title")}</h1>
              </div>
              <p className="text-sm text-muted-foreground">
                {t("localCli.subtitle", { total: tools.length, updates: updateCount })}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => void refetch()}
                disabled={isLoading}
                className="flex items-center gap-2 text-xs px-3 py-2 rounded-lg border border-border/60 hover:bg-muted/50 transition-colors disabled:opacity-50"
              >
                {isLoading ? (
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                ) : (
                  <RefreshCcw className="w-3.5 h-3.5" />
                )}
                {t("localCli.rescan")}
              </button>
              <button
                onClick={handleCheckUpdates}
                disabled={isChecking}
                className="flex items-center gap-2 text-xs px-3 py-2 rounded-lg border border-border/60 hover:bg-muted/50 transition-colors disabled:opacity-50"
              >
                {isChecking ? (
                  <Loader2 className="w-3.5 h-3.5 animate-spin" />
                ) : (
                  <RefreshCw className="w-3.5 h-3.5" />
                )}
                {isChecking ? t("localCli.checking") : t("localCli.checkUpdates")}
              </button>
            </div>
          </div>

          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-muted-foreground" />
            <input
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder={t("localCli.search")}
              className="w-full pl-9 pr-3 py-2 text-sm bg-muted/30 border border-border/60 rounded-lg focus:outline-none focus:ring-1 focus:ring-primary/50 font-mono"
            />
          </div>

          {isLoading ? (
            <div className="flex items-center justify-center h-40 text-muted-foreground gap-2 text-sm">
              <Loader2 className="w-4 h-4 animate-spin" />
              {t("localCli.loading")}
            </div>
          ) : (
            <div className="space-y-6">
              {Object.entries(groups)
                .sort(([a], [b]) => {
                  if (a === "unknown") return 1;
                  if (b === "unknown") return -1;
                  return a.localeCompare(b);
                })
                .map(([manager, items]) => (
                  <section key={manager}>
                    <h2 className="text-xs font-mono font-semibold text-muted-foreground uppercase mb-2 px-3">
                      {managerLabel(manager)} ({items.length})
                    </h2>
                    <div className="border border-border/40 rounded-xl overflow-hidden bg-card/20">
                      {items.map((tool, i) => (
                        <div
                          key={tool.id}
                          className={i > 0 ? "border-t border-border/30" : ""}
                        >
                          <LocalCliToolRow
                            tool={tool}
                            onUpdate={(id) => updateTool(id)}
                            isUpdating={isUpdating && updatingId === tool.id}
                          />
                        </div>
                      ))}
                    </div>
                  </section>
                ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
