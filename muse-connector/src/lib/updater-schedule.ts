export const INITIAL_AUTO_CHECK_DELAY_MS = 3_000;
export const FOREGROUND_AUTO_CHECK_DELAY_MS = 1_500;
export const AUTO_CHECK_SUCCESS_COOLDOWN_MS = 6 * 60 * 60 * 1000;
export const AUTO_CHECK_FAILURE_COOLDOWN_MS = 10 * 60 * 1000;

export type AutoCheckOutcome = "success" | "failure" | null;

export interface AutoCheckScheduleState {
  lastAttemptAt: number | null;
  lastOutcome: AutoCheckOutcome;
  retryOnOnlinePending: boolean;
}

export type UpdaterVisualPhase =
  | "idle"
  | "checking"
  | "up-to-date"
  | "available"
  | "downloading"
  | "verifying"
  | "installing"
  | "error";

export function computeNextAutoCheckDelayMs(
  state: AutoCheckScheduleState,
  now: number,
): number {
  if (state.lastAttemptAt === null || state.lastOutcome === null) {
    return INITIAL_AUTO_CHECK_DELAY_MS;
  }
  if (state.retryOnOnlinePending && state.lastOutcome === "failure") {
    return FOREGROUND_AUTO_CHECK_DELAY_MS;
  }
  const cooldown = state.lastOutcome === "success"
    ? AUTO_CHECK_SUCCESS_COOLDOWN_MS
    : AUTO_CHECK_FAILURE_COOLDOWN_MS;
  const elapsed = Math.max(0, now - state.lastAttemptAt);
  if (elapsed >= cooldown) {
    return FOREGROUND_AUTO_CHECK_DELAY_MS;
  }
  return Math.max(FOREGROUND_AUTO_CHECK_DELAY_MS, cooldown - elapsed);
}

export function resolvePhaseAfterSilentFailure(
  previousPhase: UpdaterVisualPhase,
  hasKnownUpdate: boolean,
): UpdaterVisualPhase {
  if (hasKnownUpdate) return "available";
  if (previousPhase === "up-to-date") return "up-to-date";
  return "idle";
}
