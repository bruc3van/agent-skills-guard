import { X } from "lucide-react";
import { useUpdate } from "../contexts/UpdateContext";
import { useTranslation } from "react-i18next";

interface UpdateBadgeProps {
  onOpenSettings?: () => void;
}

export function UpdateBadge({ onOpenSettings }: UpdateBadgeProps) {
  const { hasUpdate, updateInfo, isDismissed, dismissUpdate } = useUpdate();
  const { t } = useTranslation();

  if (!hasUpdate || isDismissed || !updateInfo) {
    return null;
  }

  return (
    <div className="flex items-center gap-2 px-3 py-1.5 bg-primary/10 border border-primary/30 rounded-md">
      <button
        type="button"
        onClick={onOpenSettings}
        className="text-xs text-primary transition-colors hover:text-primary/80"
      >
        {t("update.available")}: v{updateInfo.availableVersion}
      </button>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          dismissUpdate();
        }}
        className="text-primary/70 hover:text-primary transition-colors"
      >
        <X className="w-3.5 h-3.5" />
      </button>
    </div>
  );
}
