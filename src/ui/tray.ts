// What the tray shows, taken from the window's state so the two never disagree.
import type { TrayStatus } from "../bridge";
import type { AppState } from "./app";

export function trayStatusOf(state: AppState): TrayStatus {
  return {
    tone: state.status.tone,
    word: state.status.word,
    sentTonight: state.sentTonight,
    staging: state.staging ?? false,
    secondLine: state.awardsLine?.trim() || null,
  };
}
