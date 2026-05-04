import { createContext, useContext, useState, type ReactNode } from "react";
import type { ModuleId } from "../types";

type ModeValue = {
  activeModule: ModuleId;
  navigateTo: (id: ModuleId) => void;
};

const ModeContext = createContext<ModeValue | null>(null);

export function ModeProvider({ children }: { children: ReactNode }) {
  const [activeModule, setActiveModule] = useState<ModuleId>("inbox");

  const navigateTo = (id: ModuleId) => setActiveModule(id);

  return (
    <ModeContext.Provider value={{ activeModule, navigateTo }}>
      {children}
    </ModeContext.Provider>
  );
}

export function useMode() {
  const value = useContext(ModeContext);
  if (!value) throw new Error("useMode 必须在 <ModeProvider> 内使用");
  return value;
}
