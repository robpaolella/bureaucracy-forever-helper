import { beforeEach, describe, expect, it, vi } from "vitest";
import type { SyncStatus } from "./bridge";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const listen = vi.fn();
vi.mock("@tauri-apps/api/event", () => ({ listen }));

const { tauriBridge } = await import("./bridge");

describe("tauriBridge", () => {
  beforeEach(() => invoke.mockReset());

  it("calls each command by name with its arguments and returns the result as is", async () => {
    invoke.mockResolvedValue({ kind: "noKey" });
    expect(await tauriBridge.connect("bfh_x")).toEqual({ kind: "noKey" });
    expect(invoke).toHaveBeenLastCalledWith("connect_key", { key: "bfh_x" });
    await tauriBridge.checkStored();
    expect(invoke).toHaveBeenLastCalledWith("check_stored_key");
    await tauriBridge.removeKey();
    expect(invoke).toHaveBeenLastCalledWith("remove_stored_key");
    await tauriBridge.staging();
    expect(invoke).toHaveBeenLastCalledWith("get_staging");
    await tauriBridge.setStaging(true);
    expect(invoke).toHaveBeenLastCalledWith("set_staging_site", { on: true });
    await tauriBridge.syncStatus();
    expect(invoke).toHaveBeenLastCalledWith("get_sync_status");
  });

  it("passes on each sync status from the sync-status event until stopped", async () => {
    const stop = vi.fn();
    let emit: (event: { payload: unknown }) => void = () => {};
    listen.mockImplementation((_name, handler: typeof emit) => {
      emit = handler;
      return Promise.resolve(stop);
    });
    const heard: SyncStatus[] = [];
    const unlisten = await tauriBridge.onSyncStatus((status) => heard.push(status));
    expect(listen).toHaveBeenCalledWith("sync-status", expect.any(Function));
    emit({ payload: { kind: "noKey" } });
    emit({ payload: { kind: "idle", otherSite: 0 } });
    expect(heard).toEqual([{ kind: "noKey" }, { kind: "idle", otherSite: 0 }]);
    unlisten();
    expect(stop).toHaveBeenCalled();
  });
});
