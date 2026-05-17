import { isTauriRuntime, withTimeout } from "./base";
import { pickSaveFile } from "./dialog";

/** 打开保存对话框并导出 Markdown ZIP，返回导出页面数（用户取消返回 null）。 */
export async function exportWikiMarkdownZip(): Promise<number | null> {
  if (!isTauriRuntime()) return null;

  const dest = await pickSaveFile({
    defaultPath: "llm-wiki-markdown.zip",
    filters: [{ name: "ZIP 文件", extensions: ["zip"] }],
  });
  if (!dest) return null;

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<number>("export_wiki_markdown_zip", { destPath: dest }),
    60_000,
  );
}

/** 打开保存对话框并导出静态 HTML ZIP，返回导出页面数（用户取消返回 null）。 */
export async function exportWikiHtmlZip(): Promise<number | null> {
  if (!isTauriRuntime()) return null;

  const dest = await pickSaveFile({
    defaultPath: "llm-wiki-html.zip",
    filters: [{ name: "ZIP 文件", extensions: ["zip"] }],
  });
  if (!dest) return null;

  const { invoke } = await import("@tauri-apps/api/core");
  return withTimeout(
    invoke<number>("export_wiki_html_zip", { destPath: dest }),
    60_000,
  );
}
