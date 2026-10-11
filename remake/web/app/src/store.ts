import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { Side } from "./data";

export type Transport = "wt" | "ws";

interface Player {
  name: string;
  side: Side;
  transport: Transport;
  set: (change: Partial<Pick<Player, "name" | "side" | "transport">>) => void;
}

/** You: kept in localStorage across visits. */
export const usePlayer = create<Player>()(
  persist((set) => ({ name: "", side: "squirrels", transport: "wt", set }), {
    name: "conker-player",
  }),
);
