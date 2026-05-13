import { useEffect, useRef } from "react";
import type { FileChunk } from "../../tauri-client";
import { readFileForChat, pickFiles } from "../../tauri-client";

interface Props {
  onFileAttached: (chunk: FileChunk) => void;
  onOpenMcp: () => void;
  onOpenSkill: () => void;
  onClose: () => void;
}

export default function PlusMenu({ onFileAttached, onOpenMcp, onOpenSkill, onClose }: Props) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) onClose();
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [onClose]);

  const handlePickFile = async () => {
    const paths = await pickFiles({
      multiple: false,
      filters: [
        { name: "文档与文本", extensions: ["txt", "md", "markdown", "pdf", "doc", "docx", "pptx", "csv", "json", "yaml", "yml", "log"] },
        { name: "代码文件",   extensions: ["rs", "py", "js", "ts", "go", "java", "c", "cpp", "h"] },
        { name: "所有文件",   extensions: ["*"] },
      ],
    });
    if (!paths || paths.length === 0) return;
    try {
      const chunk = await readFileForChat(paths[0]);
      if (chunk) { onFileAttached(chunk); onClose(); }
    } catch (e) {
      console.error("读取失败:", e);
    }
  };

  return (
    <div ref={menuRef} className="plus-menu">
      <div className="plus-menu__main">
        <button className="plus-menu__item" onClick={handlePickFile}>
          <span className="plus-menu__icon">📎</span>
          <span className="plus-menu__label">
            <span className="plus-menu__label-title">上传文件</span>
            <span className="plus-menu__label-sub">txt / md / pdf / doc / docx / csv / 代码</span>
          </span>
        </button>

        <div className="plus-menu__divider" />

        <button className="plus-menu__item" onClick={() => { onOpenMcp(); onClose(); }}>
          <span className="plus-menu__icon">🔌</span>
          <span className="plus-menu__label">
            <span className="plus-menu__label-title">MCP 服务器</span>
            <span className="plus-menu__label-sub">添加 / 管理扩展工具</span>
          </span>
        </button>

        <button className="plus-menu__item" onClick={() => { onOpenSkill(); onClose(); }}>
          <span className="plus-menu__icon">⚡</span>
          <span className="plus-menu__label">
            <span className="plus-menu__label-title">Skill 技能</span>
            <span className="plus-menu__label-sub">安装 / 管理技能模板</span>
          </span>
        </button>
      </div>
    </div>
  );
}
