import { useEffect, useRef, useState } from "react";
import type { FileChunk } from "../../tauri-client";
import { readFileForChat, pickFiles } from "../../tauri-client";
import McpGallery from "./McpGallery";
import SkillInstaller from "./SkillInstaller";

type Panel = "main" | "mcp" | "skill";

interface Props {
  onFileAttached: (chunk: FileChunk) => void;
  onClose: () => void;
}

export default function PlusMenu({ onFileAttached, onClose }: Props) {
  const [panel, setPanel] = useState<Panel>("main");
  const [toast, setToast] = useState("");
  const menuRef = useRef<HTMLDivElement>(null);

  // Close on click-outside
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [onClose]);

  const showToast = (msg: string) => {
    setToast(msg);
    setTimeout(() => setToast(""), 3000);
  };

  const handlePickFile = async () => {
    const paths = await pickFiles({
      multiple: false,
      filters: [
        { name: "文档与文本", extensions: ["txt", "md", "markdown", "pdf", "docx", "pptx", "csv", "json", "yaml", "yml", "log"] },
        { name: "代码文件",   extensions: ["rs", "py", "js", "ts", "go", "java", "c", "cpp", "h"] },
        { name: "所有文件",   extensions: ["*"] },
      ],
    });
    if (!paths || paths.length === 0) return;
    try {
      const chunk = await readFileForChat(paths[0]);
      if (chunk) {
        onFileAttached(chunk);
        onClose();
      }
    } catch (e) {
      showToast(`读取失败: ${String(e)}`);
    }
  };

  return (
    <div ref={menuRef} className="plus-menu">
      {toast && <div className="plus-menu__toast">{toast}</div>}

      {panel === "main" && (
        <div className="plus-menu__main">
          <button className="plus-menu__item" onClick={handlePickFile}>
            <span className="plus-menu__icon">📎</span>
            <span className="plus-menu__label">
              <span className="plus-menu__label-title">上传文件</span>
              <span className="plus-menu__label-sub">txt / md / pdf / docx / csv / 代码</span>
            </span>
          </button>

          <div className="plus-menu__divider" />

          <button className="plus-menu__item" onClick={() => setPanel("mcp")}>
            <span className="plus-menu__icon">🔌</span>
            <span className="plus-menu__label">
              <span className="plus-menu__label-title">添加 MCP 服务器</span>
              <span className="plus-menu__label-sub">从精选列表一键添加工具</span>
            </span>
          </button>

          <button className="plus-menu__item" onClick={() => setPanel("skill")}>
            <span className="plus-menu__icon">⚡</span>
            <span className="plus-menu__label">
              <span className="plus-menu__label-title">安装 Skill</span>
              <span className="plus-menu__label-sub">从 URL 下载技能模板</span>
            </span>
          </button>
        </div>
      )}

      {panel === "mcp" && (
        <McpGallery
          onClose={onClose}
          onAdded={(name) => { showToast(`✓ ${name} 已添加并加载`); setPanel("main"); }}
        />
      )}

      {panel === "skill" && (
        <SkillInstaller
          onClose={onClose}
          onInstalled={(name) => { showToast(`✓ Skill "${name}" 已安装`); setPanel("main"); }}
        />
      )}
    </div>
  );
}
