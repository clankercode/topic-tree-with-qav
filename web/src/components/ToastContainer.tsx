import { useToastStore } from "../store/toast";
import { X } from "lucide-react";

export function ToastContainer() {
  const { toasts, removeToast } = useToastStore();

  if (toasts.length === 0) return null;

  return (
    <div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={`flex items-center gap-3 rounded border px-4 py-3 shadow-lg ${
            toast.type === "error"
              ? "border-red-500/50 bg-red-950/90 text-red-200"
              : toast.type === "success"
                ? "border-green-500/50 bg-green-950/90 text-green-200"
                : "border-[rgb(var(--border))] bg-[rgb(var(--surface))] text-[rgb(var(--foreground))]"
          }`}
          role="alert"
        >
          <span className="text-sm">{toast.message}</span>
          <button
            onClick={() => removeToast(toast.id)}
            className="ml-2 rounded p-1 opacity-70 hover:opacity-100"
            aria-label="Dismiss"
          >
            <X className="h-3 w-3" />
          </button>
        </div>
      ))}
    </div>
  );
}
