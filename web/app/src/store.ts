import { create } from "zustand";
import { persist } from "zustand/middleware";
import { randomPseudonym, type Side } from "./data";

export type Transport = "wt" | "ws";

interface Player {
  /** The pseudonym over your head in the match: a random one until you change it. */
  name: string;
  side: Side;
  transport: Transport;
  /** The pick asked for (data.ts `PICKS`' index), where the server's map and mode allow it; null:
   * the server's default for the slot. */
  character: number | null;
  set: (change: Partial<Pick<Player, "name" | "side" | "transport" | "character">>) => void;
}

/** You: kept in localStorage across visits. */
export const usePlayer = create<Player>()(
  persist(
    (set) => ({
      name: randomPseudonym(),
      side: "squirrels",
      transport: "wt",
      character: null,
      set,
    }),
    { name: "conker-player" },
  ),
);
// persist writes only on a change: the first visit's random pseudonym is kept from here.
usePlayer.setState({});
