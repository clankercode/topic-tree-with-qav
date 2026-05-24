import { create } from "zustand";

export interface Toast {
  id: string;
  message: string;
  type: "error" | "info" | "success";
}

interface ToastState {
  toasts: Toast[];
  addToast(message: string, type?: Toast["type"]): void;
  removeToast(id: string): void;
}

export const useToastStore = create<ToastState>((set) => ({
  toasts: [],
  addToast(message, type = "error") {
    const id = crypto.randomUUID();
    set((state) => ({
      toasts: [...state.toasts, { id, message, type }],
    }));
    setTimeout(() => {
      set((state) => ({
        toasts: state.toasts.filter((t) => t.id !== id),
      }));
    }, 4000);
  },
  removeToast(id) {
    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    }));
  },
}));
