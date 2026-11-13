/** The installer window's link to Cilica Setup: what is on this PC, and the running job. */
import { useCallback, useEffect, useReducer, useState } from "react";
import { toAppError } from "@/ipc/client";
import {
  type InstallOptions,
  onJobEvent,
  type SetupInfo,
  setupApi,
  type UninstallOptions,
} from "@/ipc/installer/client";
import type { AppError } from "@/ipc/types";
import { initialJob, jobReducer } from "./flow";

/** Setup's facts for the first screen, read once. */
export function useSetupInfo() {
  const [info, setInfo] = useState<SetupInfo | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  useEffect(() => {
    let live = true;
    setupApi
      .getSetupInfo()
      .then((value) => live && setInfo(value))
      .catch((e: unknown) => live && setError(toAppError(e)));
    return () => {
      live = false;
    };
  }, []);
  return { info, error };
}

/** Start an install or uninstall and follow it to the end. */
export function useJob() {
  const [state, dispatch] = useReducer(jobReducer, initialJob);
  useEffect(() => onJobEvent((event) => dispatch({ type: "event", event })), []);

  // Setup may report its first step before the command returns, so the screen changes first.
  const install = useCallback((options: InstallOptions) => {
    dispatch({ type: "starting" });
    setupApi
      .install(options)
      .then((steps) => dispatch({ type: "started", steps }))
      .catch((e: unknown) => dispatch({ type: "refused", error: toAppError(e) }));
  }, []);

  const uninstall = useCallback((options: UninstallOptions) => {
    dispatch({ type: "starting" });
    setupApi
      .uninstall(options)
      .then((steps) => dispatch({ type: "started", steps }))
      .catch((e: unknown) => dispatch({ type: "refused", error: toAppError(e) }));
  }, []);

  const retry = useCallback(() => dispatch({ type: "retry" }), []);

  return { state, install, uninstall, retry };
}
