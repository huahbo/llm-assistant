import { createContext, useContext, useMemo, type ReactNode } from "react";
import { isTauriRuntime } from "../tauri-client";

type RuntimeValue = {
  isTauri: boolean;
};

const RuntimeContext = createContext<RuntimeValue | null>(null);

export function RuntimeProvider({ children }: { children: ReactNode }) {
  const value = useMemo<RuntimeValue>(() => ({ isTauri: isTauriRuntime() }), []);
  return <RuntimeContext.Provider value={value}>{children}</RuntimeContext.Provider>;
}

export function useRuntime() {
  const value = useContext(RuntimeContext);
  if (!value) throw new Error("useRuntime 必须在 <RuntimeProvider> 内使用");
  return value;
}
