import type { GoogleConnectorStatus } from "./contracts";

export const SETUP_PREFERENCE_KEY = "kyra.setup.v1";

export type SetupPreference = "completed" | "skipped";

export function readSetupPreference(storage: Pick<Storage, "getItem"> = window.localStorage): SetupPreference | null {
  try {
    const value = storage.getItem(SETUP_PREFERENCE_KEY);
    return value === "completed" || value === "skipped" ? value : null;
  } catch {
    return null;
  }
}

export function writeSetupPreference(
  preference: SetupPreference,
  storage: Pick<Storage, "setItem"> = window.localStorage,
) {
  try {
    storage.setItem(SETUP_PREFERENCE_KEY, preference);
  } catch {
    // Setup remains usable when storage is unavailable; it simply will not persist.
  }
}

export function shouldShowSetup(status: GoogleConnectorStatus, preference: SetupPreference | null) {
  return status.state === "disconnected" && preference === null;
}
