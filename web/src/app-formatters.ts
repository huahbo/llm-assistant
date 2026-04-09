import type { BackendAppMode, LogEntry } from "./types";

export const formatBackendMode = (mode: BackendAppMode) =>
  mode === "StrictLocal" ? "Strict Local" : "Hybrid";

export const formatLogLevel = (level: LogEntry["level"]) => {
  if (level === "Warn") {
    return "Warn";
  }

  if (level === "Error") {
    return "Error";
  }

  return "Info";
};
