export const RECENT_VAULT_PATHS_STORAGE_KEY = "llm_wiki_recent_vault_paths_v1";
const RECENT_VAULT_PATHS_MAX = 8;

export const normalizeRecentVaultPaths = (paths: string[]): string[] => {
  const seen = new Set<string>();
  const normalized: string[] = [];
  for (const rawPath of paths) {
    const path = rawPath.trim();
    if (!path) continue;
    const key = path.replace(/\\/g, "/").toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    normalized.push(path);
  }
  return normalized;
};

export const mergeRecentVaultPaths = (nextPath: string, existing: string[]): string[] =>
  normalizeRecentVaultPaths([nextPath, ...existing]).slice(0, RECENT_VAULT_PATHS_MAX);

export const readRecentVaultPathsFromStorage = (): string[] => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) return [];
    const rawValue = storage.getItem(RECENT_VAULT_PATHS_STORAGE_KEY);
    if (!rawValue) return [];
    const parsed = JSON.parse(rawValue);
    if (!Array.isArray(parsed)) return [];
    return normalizeRecentVaultPaths(parsed.filter((item): item is string => typeof item === "string"))
      .slice(0, RECENT_VAULT_PATHS_MAX);
  } catch {
    return [];
  }
};

export const writeRecentVaultPathsToStorage = (paths: string[]): void => {
  try {
    const storage = globalThis.localStorage;
    if (!storage) return;
    const normalized = normalizeRecentVaultPaths(paths).slice(0, RECENT_VAULT_PATHS_MAX);
    storage.setItem(RECENT_VAULT_PATHS_STORAGE_KEY, JSON.stringify(normalized));
  } catch (err) {
    console.warn("[llm-wiki] recentVaultPaths 持久化失败:", err);
  }
};
