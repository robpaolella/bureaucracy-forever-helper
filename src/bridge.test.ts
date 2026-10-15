import { beforeEach, describe, expect, it, vi } from "vitest";
import type { RaidsView, SyncStatus } from "./bridge";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
const listen = vi.fn();
vi.mock("@tauri-apps/api/event", () => ({ listen }));

const open = vi.fn();
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));

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
    await tauriBridge.raids();
    expect(invoke).toHaveBeenLastCalledWith("get_raids");
    await tauriBridge.searchFolder();
    expect(invoke).toHaveBeenLastCalledWith("search_game_folder");
    await tauriBridge.checkPickedFolder("/games");
    expect(invoke).toHaveBeenLastCalledWith("check_picked_game_folder", { folder: "/games" });
    await tauriBridge.rememberVersion("/games/_retail_");
    expect(invoke).toHaveBeenLastCalledWith("remember_game_version", { folder: "/games/_retail_" });
    await tauriBridge.rememberedFolder();
    expect(invoke).toHaveBeenLastCalledWith("get_remembered_game_folder");
  });

  it("opens the native picker for one folder and gives back its path, or null when cancelled", async () => {
    open.mockResolvedValueOnce("/games/World of Warcraft");
    expect(await tauriBridge.pickFolder()).toBe("/games/World of Warcraft");
    expect(open).toHaveBeenLastCalledWith({ directory: true, multiple: false });
    open.mockResolvedValueOnce(null);
    expect(await tauriBridge.pickFolder()).toBeNull();
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

  it("passes on each raids view from the raids event until stopped", async () => {
    const stop = vi.fn();
    let emit: (event: { payload: unknown }) => void = () => {};
    listen.mockImplementation((_name, handler: typeof emit) => {
      emit = handler;
      return Promise.resolve(stop);
    });
    const heard: RaidsView[] = [];
    const unlisten = await tauriBridge.onRaids((view) => heard.push(view));
    expect(listen).toHaveBeenCalledWith("raids", expect.any(Function));
    emit({ payload: { state: { kind: "loading" }, raids: [] } });
    expect(heard).toEqual([{ state: { kind: "loading" }, raids: [] }]);
    unlisten();
    expect(stop).toHaveBeenCalled();
  });
});
