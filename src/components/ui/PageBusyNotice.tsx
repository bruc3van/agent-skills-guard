import { Loader2 } from "lucide-react";

interface PageBusyNoticeProps {
  message: string;
}

export function PageBusyNotice({ message }: PageBusyNoticeProps) {
  return (
    <div
      role="status"
      aria-live="polite"
      className="flex items-center gap-2 rounded-2xl border border-primary/15 bg-primary/5 px-4 py-3 text-sm text-primary"
    >
      <Loader2 className="h-4 w-4 animate-spin" />
      <span>{message}</span>
    </div>
  );
}
