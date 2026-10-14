// The chain list's data. Each feature issue fills in its own row; the list shows them
// and opens the first one that is broken.
import type { ButtonVariant, Tone } from "./components";

export type RowId = "site" | "folder" | "addon" | "reserves" | "awards";

/** The order the design lists them, from the site to the awards. */
export const ROW_ORDER: readonly RowId[] = ["site", "folder", "addon", "reserves", "awards"];

export const ROW_NAMES: Record<RowId, string> = {
  site: "Guild site",
  folder: "Game folder",
  addon: "Add-on",
  reserves: "Reserves",
  awards: "Awards",
};

export interface RowMessage {
  text: string;
  action?: { label: string; variant: ButtonVariant; onClick?: () => void };
}

export interface ChainRow {
  tone: Tone;
  /** The pill, such as "Connected". */
  word: string;
  /** The line beside the name. */
  detail: string;
  /** Marks the row as the thing to fix; the first such row opens with its message. */
  broken?: boolean;
  message?: RowMessage;
  /** Extra lines under the row, such as each raid's start time. */
  under?: Node[];
}

export type ChainState = Record<RowId, ChainRow>;

/** The one row to open: the first broken one, or none when nothing is broken. */
export function firstBroken(chain: ChainState): RowId | undefined {
  return ROW_ORDER.find((id) => chain[id].broken);
}
