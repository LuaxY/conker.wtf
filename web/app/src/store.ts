import { create } from "zustand";
import { persist } from "zustand/middleware";
import type { Side } from "./data";

export type Transport = "wt" | "ws";

interface Player {
  side: Side;
  transport: Transport;
  /** The pick asked for (data.ts `PICKS`' index), where the server's map and mode allow it; null:
   * the server's default for the slot. */
  character: number | null;
  set: (change: Partial<Pick<Player, "side" | "transport" | "character">>) => void;
}

/** You: kept in localStorage across visits. */
export const usePlayer = create<Player>()(
  persist((set) => ({ side: "squirrels", transport: "wt", character: null, set }), {
    name: "conker-player",
  }),
);
