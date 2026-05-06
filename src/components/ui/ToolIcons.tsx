import { useState } from "react";
import { Globe, Loader2 } from "lucide-react";
import type { FC, SVGProps } from "react";

type IconComponent = FC<SVGProps<SVGSVGElement> & { size?: number }>;

// 直接导入各图标的 Color 变体，避免 TS 对 CompoundedIcon 属性推断的限制
import ClaudeCodeColor from "@lobehub/icons/es/ClaudeCode/components/Color";
import CodexColor from "@lobehub/icons/es/Codex/components/Color";
import AntigravityColor from "@lobehub/icons/es/Antigravity/components/Color";
import OpenCodeMono from "@lobehub/icons/es/OpenCode/components/Mono";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "./alert-dialog";

interface ToolIconsProps {
  /** 当前展示为已激活的工具 id 列表（如 ["agents","claude-code","codex"]） */
  activeToolIds: string[];
  /** 是否为本地直放 skill（本地 skill 只展示、不可操作） */
  isLocalOnly?: boolean;
  /** 点击图标触发：toolId, active → void */
  onToggle: (toolId: string, active: boolean) => void;
  /** 是否正在操作（禁用交互） */
  disabled?: boolean;
  /** 当前正在切换的 toolId（显示 loading） */
  pendingToolId?: string | null;
}

interface ToolDef {
  id: string;
  label: string;
  Icon: IconComponent;
  color: string;
  bg: string;
}

const TOOLS: ToolDef[] = [
  {
    id: "claude-code",
    label: "Claude Code",
    Icon: ClaudeCodeColor as IconComponent,
    color: "#D97757",
    bg: "#D9775715",
  },
  {
    id: "codex",
    label: "Codex",
    Icon: CodexColor as IconComponent,
    color: "#4d9de0",
    bg: "#4d9de015",
  },
  {
    id: "antigravity",
    label: "Antigravity",
    Icon: AntigravityColor as IconComponent,
    color: "#8b5cf6",
    bg: "#8b5cf615",
  },
  {
    id: "opencode",
    label: "OpenCode",
    Icon: OpenCodeMono as IconComponent,
    color: "#374151",
    bg: "#37415115",
  },
];

export function ToolIcons({
  activeToolIds,
  isLocalOnly = false,
  onToggle,
  disabled = false,
  pendingToolId = null,
}: ToolIconsProps) {
  const [confirmTarget, setConfirmTarget] = useState<ToolDef | null>(null);

  function handleClick(tool: ToolDef, active: boolean) {
    if (disabled) return;
    if (active) {
      // 取消链接 → 需要确认
      setConfirmTarget(tool);
    } else {
      // 新增同步 → 直接执行
      onToggle(tool.id, false);
    }
  }

  return (
    <>
      <div className="pt-4 border-t border-border/60">
        <div className="text-xs font-medium text-muted-foreground mb-3">
          编程工具
        </div>
        <div className="flex flex-wrap gap-2">
          {activeToolIds.includes("agents") && (
            <div
              className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border border-emerald-500/50 bg-emerald-500/10"
              title="Universal (.agents)"
            >
              <Globe className="w-4 h-4 text-emerald-500" />
              <span className="text-xs text-emerald-600">.agents</span>
            </div>
          )}

          {TOOLS.map((tool) => {
            const active = activeToolIds.includes(tool.id);
            const isPending = pendingToolId === tool.id;
            const interactionDisabled = disabled || isPending;

            return (
              <button
                key={tool.id}
                type="button"
                onClick={() => handleClick(tool, active)}
                disabled={interactionDisabled}
                aria-disabled={interactionDisabled}
                title={
                  isLocalOnly
                    ? active
                      ? `已在 ${tool.label}，点击取消`
                      : `点击同步到 ${tool.label}（将移至通用目录）`
                    : active
                    ? `已同步到 ${tool.label}，点击取消`
                    : `点击同步到 ${tool.label}`
                }
                className={`
                  flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border transition-all cursor-pointer
                  ${
                    active
                      ? "border-current shadow-sm"
                      : "border-border/60 opacity-50 hover:opacity-80"
                  }
                  ${interactionDisabled ? "opacity-30" : ""}
                `}
                style={
                  active
                    ? { borderColor: tool.color, backgroundColor: tool.bg, color: tool.color }
                    : undefined
                }
              >
                {isPending ? (
                  <Loader2 className="w-4 h-4 animate-spin" />
                ) : (
                  <tool.Icon
                    size={16}
                    className={active ? "" : "grayscale"}
                    style={!active ? { filter: "grayscale(1)", opacity: 0.5 } : undefined}
                  />
                )}
                <span className="text-xs font-medium">{tool.label}</span>
              </button>
            );
          })}
        </div>
      </div>

      {/* 取消同步确认对话框 */}
      <AlertDialog
        open={!!confirmTarget}
        onOpenChange={(open) => {
          if (!open) setConfirmTarget(null);
        }}
      >
        <AlertDialogContent className="max-w-sm">
          <AlertDialogHeader>
            <AlertDialogTitle>取消同步</AlertDialogTitle>
            <AlertDialogDescription>
              将从 {confirmTarget?.label} 移除该 skill 的链接，skill 本身不会被删除。
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <button
              className="px-4 py-2 text-sm rounded-md border hover:bg-muted/50 transition-colors"
              onClick={() => setConfirmTarget(null)}
            >
              取消
            </button>
            <button
              className="px-4 py-2 text-sm rounded-md bg-red-500 text-white hover:bg-red-600 transition-colors"
              onClick={() => {
                if (confirmTarget) {
                  onToggle(confirmTarget.id, true);
                  setConfirmTarget(null);
                }
              }}
            >
              移除同步
            </button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
