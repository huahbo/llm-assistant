import { isTauriRuntime, withTimeout } from "./base";
import { pickSaveFile } from "./dialog";
import { saveResearchDoc } from "./search";

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

/** 将研究报告 .md 转为独立 HTML，弹保存对话框，用户选路径后写入。取消返回 false。 */
export async function exportResearchHtml(savedMdPath: string, topic: string): Promise<boolean> {
  if (!isTauriRuntime()) return false;
  const { invoke } = await import("@tauri-apps/api/core");
  const htmlContent = await withTimeout(
    invoke<string>("research_md_to_html", { mdPath: savedMdPath }),
    30_000,
  );
  const safeName = topic.replace(/[\\/:*?"<>|]/g, "-").trim().slice(0, 40) || "research";
  const dest = await pickSaveFile({
    defaultPath: `${safeName}.html`,
    filters: [{ name: "HTML 文件", extensions: ["html"] }],
  });
  if (!dest) return false;
  await saveResearchDoc(dest, htmlContent);
  return true;
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
