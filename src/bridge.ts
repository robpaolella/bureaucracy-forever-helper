// The only place the front-end calls Tauri. Everything else takes a `Bridge`, so tests
// can hand it a fake. The result shapes mirror `KeyOutcome` and `ChangeOutcome` in
// src-tauri/src/commands.rs.
import { invoke } from "@tauri-apps/api/core";

export type KeyOutcome =
  | {
      kind: "accepted";
      /** The key owner's Discord name, for "Connected as <name>". */
      officer: string;
      env: string;
      formatVersions: number[];
      addonVersion: string;
      addonUrl: string;
    }
  | { kind: "refused"; reason: "badKey" | "notOfficer"; message: string }
  | { kind: "siteOff"; message: string }
  | { kind: "stagingNotOpen"; message: string }
  | { kind: "unreachable"; message: string }
  | { kind: "storeFailed"; message: string }
  | { kind: "malformed"; reason: "empty" | "malformed"; message: string }
  /** Checking the stored key, but none is stored for the current site. */
  | { kind: "noKey" };

export type ChangeOutcome =
  | { kind: "done" }
  | { kind: "storeFailed"; message: string }
  | { kind: "settingsFailed"; message: string };

/** Each call rejects only if the Rust side itself failed; every engine result is data. */
export interface Bridge {
  connect(key: string): Promise<KeyOutcome>;
  checkStored(): Promise<KeyOutcome>;
  removeKey(): Promise<ChangeOutcome>;
  staging(): Promise<boolean>;
  setStaging(on: boolean): Promise<ChangeOutcome>;
}

export const tauriBridge: Bridge = {
  connect: (key) => invoke("connect_key", { key }),
  checkStored: () => invoke("check_stored_key"),
  removeKey: () => invoke("remove_stored_key"),
  staging: () => invoke("get_staging"),
  setStaging: (on) => invoke("set_staging_site", { on }),
};
