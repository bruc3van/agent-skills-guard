import { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import { useAgentTools } from "@/lib/agent-tools";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "./alert-dialog";

interface ToolSyncDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  /** Pre-selected tool ids (linked_tools of the skill, or empty for batch) */
  initialSelected?: string[];
  /** Called with the selected tool ids (excluding "agents") when user confirms */
  onConfirm: (tools: string[]) => void;
  loading?: boolean;
}

export function ToolSyncDialog({
  open,
  onOpenChange,
  title,
  description,
  initialSelected = [],
  onConfirm,
  loading = false,
}: ToolSyncDialogProps) {
  const { t } = useTranslation();
  const { data: tools = [] } = useAgentTools();
  const [selected, setSelected] = useState<Set<string>>(new Set(initialSelected));

  useEffect(() => {
    if (open) {
      setSelected(new Set(initialSelected));
    }
  }, [open, initialSelected.join(",")]);

  function toggle(id: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  function handleConfirm() {
    onConfirm(Array.from(selected).filter((id) => id !== "agents"));
  }

  const nonAgentTools = tools.filter((t) => t.id !== "agents");

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent className="max-w-md">
        <AlertDialogHeader>
          <AlertDialogTitle>{title}</AlertDialogTitle>
          {description && <AlertDialogDescription>{description}</AlertDialogDescription>}
        </AlertDialogHeader>

        <div className="py-2 space-y-2">
          {/* Agents (source) — always on, readonly */}
          <label className="flex items-center gap-3 px-3 py-2 rounded-lg bg-muted/50 opacity-70 cursor-not-allowed select-none">
            <input type="checkbox" checked readOnly className="accent-primary" />
            <span className="flex-1 text-sm font-medium">Universal (.agents)</span>
            <span className="text-xs text-muted-foreground">唯一源</span>
          </label>

          {nonAgentTools.map((tool) => (
            <label
              key={tool.id}
              className={`flex items-center gap-3 px-3 py-2 rounded-lg border transition-colors cursor-pointer ${
                selected.has(tool.id)
                  ? "border-primary/50 bg-primary/5"
                  : "border-border hover:border-border/80"
              }`}
            >
              <input
                type="checkbox"
                checked={selected.has(tool.id)}
                onChange={() => toggle(tool.id)}
                className="accent-primary"
              />
              <span className="flex-1 text-sm font-medium">{tool.label}</span>
              {tool.path && (
                <span className="text-xs text-muted-foreground font-mono truncate max-w-[140px]">
                  {tool.path.replace(/\\/g, "/").replace(/^.*?(?=\/)/, "~")}
                </span>
              )}
            </label>
          ))}

          {nonAgentTools.length === 0 && (
            <p className="text-sm text-muted-foreground text-center py-4">未检测到其他工具目录</p>
          )}
        </div>

        <AlertDialogFooter>
          <button
            className="px-4 py-2 text-sm rounded-md border hover:bg-muted/50 transition-colors"
            onClick={() => onOpenChange(false)}
            disabled={loading}
          >
            {t("common.cancel", "取消")}
          </button>
          <button
            className="px-4 py-2 text-sm rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors flex items-center gap-2 disabled:opacity-50"
            onClick={handleConfirm}
            disabled={loading}
          >
            {loading && <Loader2 className="h-3 w-3 animate-spin" />}
            {t("common.confirm", "确认")}
          </button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
