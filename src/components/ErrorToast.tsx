import { X } from "lucide-react";

interface ErrorToastProps {
  message: string;
  onDismiss: () => void;
}

export function ErrorToast({ message, onDismiss }: ErrorToastProps) {
  if (!message) return null;

  return (
    <div className="absolute top-4 right-4 bg-red-900/95 text-red-200 text-xs px-3 py-2 rounded-lg border border-red-700 shadow-lg animate-in slide-in-from-right fade-in z-50 flex items-center gap-2 max-w-md">
      <span className="flex-1">{message}</span>
      <button
        onClick={onDismiss}
        className="p-1 hover:bg-red-800 rounded transition shrink-0"
        title="Dismiss"
      >
        <X size={12} />
      </button>
    </div>
  );
}
