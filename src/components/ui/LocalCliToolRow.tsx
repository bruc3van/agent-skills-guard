import { Loader2, RefreshCw, Terminal } from "lucide-react";
import type { LocalCliTool } from "../../types";
import { canAutoUpdate } from "../../lib/local-cli";

interface Props {
  tool: LocalCliTool;
  onUpdate: (id: string) => void;
  isUpdating: boolean;
}

export function LocalCliToolRow({ tool, onUpdate, isUpdating }: Props) {
  const hasUpdate = tool.update_available;
  const status = tool.update_status;

  return (
    <div className="flex items-center gap-3 py-2.5 px-3 rounded-lg hover:bg-muted/30 transition-colors group">
      <div className="w-7 h-7 rounded-md bg-primary/10 flex items-center justify-center flex-shrink-0">
        <Terminal className="w-3.5 h-3.5 text-primary" />
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-sm font-mono font-medium truncate">{tool.id}</span>
          {tool.current_version && (
            <span className="text-xs text-muted-foreground font-mono">
              v{tool.current_version}
            </span>
          )}
          {hasUpdate && tool.latest_version && (
            <span className="text-[10px] bg-amber-500/15 text-amber-600 border border-amber-500/40 px-1.5 py-0.5 rounded font-mono">
              → v{tool.latest_version}
            </span>
          )}
          {status === "success" && (
            <span className="text-[10px] bg-emerald-500/15 text-emerald-600 border border-emerald-500/40 px-1.5 py-0.5 rounded font-mono">
              已更新
            </span>
          )}
          {status === "failed" && (
            <span className="text-[10px] bg-red-500/15 text-red-600 border border-red-500/40 px-1.5 py-0.5 rounded font-mono">
              更新失败
            </span>
          )}
        </div>
        <div className="text-[11px] text-muted-foreground/60 font-mono truncate">
          {tool.detected_path}
        </div>
      </div>

      {hasUpdate && canAutoUpdate(tool) && (
        <button
          onClick={() => onUpdate(tool.id)}
          disabled={isUpdating}
          className="flex items-center gap-1 text-xs px-2.5 py-1.5 rounded-md border border-border/60 hover:bg-muted/50 transition-colors disabled:opacity-50 opacity-0 group-hover:opacity-100"
        >
          {isUpdating ? (
            <Loader2 className="w-3 h-3 animate-spin" />
          ) : (
            <RefreshCw className="w-3 h-3" />
          )}
          更新
        </button>
      )}
    </div>
  );
}
