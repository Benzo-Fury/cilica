import { beforeEach, describe, expect, it } from "vitest";
import { toastStore } from "@/lib/toast";
import { checkForUpdate, installUpdate, useUpdateState } from "./hooks";

const reset = () => useUpdateState.setState({ phase: "idle", update: null, error: null });

describe("app updates", () => {
  beforeEach(() => {
    reset();
    window.history.replaceState(null, "", "/");
  });

  it("reports up to date when no release is newer", async () => {
    await checkForUpdate({ manual: true, autoInstall: true });
    expect(useUpdateState.getState().phase).toBe("current");
  });

  it("installs in the background and offers a restart when automatic updates are on", async () => {
    window.history.replaceState(null, "", "/?update");
    await checkForUpdate({ manual: false, autoInstall: true });
    const state = useUpdateState.getState();
    expect(state.phase).toBe("ready");
    expect(state.update?.version).toBe("0.2.0");
    expect(toastStore.getSnapshot().toasts.some((t) => t.title === "Cilica 0.2.0 is ready")).toBe(
      true,
    );
  });

  it("waits for the person when automatic updates are off", async () => {
    window.history.replaceState(null, "", "/?update");
    reset();
    await checkForUpdate({ manual: false, autoInstall: false });
    expect(useUpdateState.getState().phase).toBe("available");
    await installUpdate();
    expect(useUpdateState.getState().phase).toBe("ready");
  });
});
