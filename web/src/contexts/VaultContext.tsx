import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { mergeRecentVaultPaths, readRecentVaultPathsFromStorage, writeRecentVaultPathsToStorage } from "../vault-utils";

export const DEFAULT_VAULT_PATH = "vault";

type VaultValue = {
  vaultPath: string;
  recentVaultPaths: string[];
  setVaultPath: (path: string) => void;
  setRecentVaultPaths: (paths: string[]) => void;
  pushRecentVaultPath: (path: string) => void;
};

const VaultContext = createContext<VaultValue | null>(null);

export function VaultProvider({ children }: { children: ReactNode }) {
  const [vaultPath, setVaultPath] = useState(DEFAULT_VAULT_PATH);
  const [recentVaultPaths, setRecentVaultPaths] = useState<string[]>(() =>
    readRecentVaultPathsFromStorage(),
  );

  useEffect(() => {
    writeRecentVaultPathsToStorage(recentVaultPaths);
  }, [recentVaultPaths]);

  const pushRecentVaultPath = (path: string) => {
    setRecentVaultPaths((prev) => mergeRecentVaultPaths(path, prev));
  };

  return (
    <VaultContext.Provider value={{ vaultPath, recentVaultPaths, setVaultPath, setRecentVaultPaths, pushRecentVaultPath }}>
      {children}
    </VaultContext.Provider>
  );
}

export function useVault() {
  const value = useContext(VaultContext);
  if (!value) throw new Error("useVault 必须在 <VaultProvider> 内使用");
  return value;
}
