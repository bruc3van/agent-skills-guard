import { AlertTriangle, Loader2, Trash2 } from "lucide-react";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "./ui/alert-dialog";

type SkillUninstallConfirmDialogLabels = {
  title: string;
  description: string;
  impact: string;
  cancel: string;
  confirm: string;
  confirming: string;
};

type SkillUninstallConfirmDialogProps = {
  open: boolean;
  skillName: string;
  operationCount: number;
  pathCount: number;
  isConfirming: boolean;
  labels: SkillUninstallConfirmDialogLabels;
  onCancel: () => void;
  onConfirm: () => void;
};

export function SkillUninstallConfirmDialog({
  open,
  skillName,
  operationCount,
  pathCount,
  isConfirming,
  labels,
  onCancel,
  onConfirm,
}: SkillUninstallConfirmDialogProps) {
  return (
    <AlertDialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onCancel();
      }}
    >
      <AlertDialogContent
        className="max-w-md"
        aria-label={`${labels.title}: ${skillName}`}
        data-operation-count={operationCount}
        data-path-count={pathCount}
      >
        <AlertDialogHeader>
          <AlertDialogTitle className="flex items-center gap-2">
            <AlertTriangle className="h-4 w-4 text-destructive" />
            {labels.title}
          </AlertDialogTitle>
          <AlertDialogDescription asChild>
            <div className="space-y-3">
              <p>{labels.description}</p>
              <div className="rounded-lg border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
                {labels.impact}
              </div>
            </div>
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isConfirming} onClick={onCancel}>
            {labels.cancel}
          </AlertDialogCancel>
          <button
            type="button"
            onClick={onConfirm}
            disabled={isConfirming}
            className="apple-button-destructive h-9 px-4 text-sm flex items-center gap-2 disabled:opacity-50"
          >
            {isConfirming ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Trash2 className="h-4 w-4" />
            )}
            {isConfirming ? labels.confirming : labels.confirm}
          </button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
