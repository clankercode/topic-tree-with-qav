import { create } from "zustand";

interface FollowHostState {
  followingHost: boolean;
  setFollowingHost(following: boolean): void;
  init(): void;
}

const STORAGE_KEY = "followingHost";

export const useFollowHostStore = create<FollowHostState>((set) => ({
  followingHost: false,

  setFollowingHost(following: boolean) {
    localStorage.setItem(STORAGE_KEY, String(following));
    set({ followingHost: following });
  },

  init() {
    const saved = localStorage.getItem(STORAGE_KEY);
    const followingHost = saved === "true";
    set({ followingHost });
  },
}));