import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { Side } from "./data";

export type Transport = "wt" | "ws";

interface Player {
  side: Side;
  transport: Transport;
  set: (change: Partial<Pick<Player, "side" | "transport">>) => void;
}

/** You: kept in localStorage across visits. */
export const usePlayer = create<Player>()(
  persist((set) => ({ side: "squirrels", transport: "wt", set }), {
    name: "conker-player",
  }),
);
