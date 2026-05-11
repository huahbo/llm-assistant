import { isTauriRuntime } from "./base";

/** 打开文件选择对话框，返回选中路径数组（取消返回 null） */
export async function pickFiles(options: {
  multiple?: boolean;
  filters?: Array<{ name: string; extensions: string[] }>;
}): Promise<string[] | null> {
  if (!isTauriRuntime()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const result = await open({
    multiple: options.multiple ?? false,
    filters: options.filters,
  });
  if (!result) return null;
  if (Array.isArray(result)) return result as string[];
  return [result as string];
}

/** 打开文件夹选择对话框，返回选中路径（取消返回 null） */
export async function pickFolder(): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  const { open } = await import("@tauri-apps/plugin-dialog");
  const result = await open({ directory: true, multiple: false });
  if (!result) return null;
  return Array.isArray(result) ? (result[0] as string) : (result as string);
}

/** 打开保存文件对话框，返回保存路径（取消返回 null） */
export async function pickSaveFile(options: {
  defaultPath?: string;
  filters?: Array<{ name: string; extensions: string[] }>;
}): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  const { save } = await import("@tauri-apps/plugin-dialog");
  const result = await save({
    defaultPath: options.defaultPath,
    filters: options.filters,
  });
  if (!result) return null;
  return result as string;
}

/** 打开确认对话框（Tauri 原生）。浏览器环境回退到 window.confirm。 */
export async function askConfirmDialog(
  message: string,
  options?: {
    title?: string;
    kind?: "info" | "warning" | "error";
    okLabel?: string;
    cancelLabel?: string;
  },
): Promise<boolean> {
  if (!isTauriRuntime()) {
    return globalThis.confirm(message);
  }
  const { confirm } = await import("@tauri-apps/plugin-dialog");
  return await confirm(message, {
    title: options?.title,
    kind: options?.kind,
    okLabel: options?.okLabel,
    cancelLabel: options?.cancelLabel,
  });
}
