import { useState } from "react";
import { marked } from "marked";
import DOMPurify from "dompurify";
import hljs from "highlight.js";
import "highlight.js/styles/atom-one-dark.min.css";

interface Segment {
  type: "markdown" | "code";
  lang?: string;
  content: string;
}

function parseSegments(text: string): Segment[] {
  const segments: Segment[] = [];
  const re = /```(\w*)\r?\n([\s\S]*?)```/g;
  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = re.exec(text)) !== null) {
    if (match.index > lastIndex) {
      const md = text.slice(lastIndex, match.index);
      if (md.trim()) segments.push({ type: "markdown", content: md });
    }
    segments.push({ type: "code", lang: match[1] || "plaintext", content: match[2].trimEnd() });
    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < text.length) {
    const tail = text.slice(lastIndex);
    if (tail.trim()) segments.push({ type: "markdown", content: tail });
  }

  return segments.length > 0 ? segments : [{ type: "markdown", content: text }];
}

function MarkdownBlock({ content }: { content: string }) {
  const raw = marked.parse(content) as string;
  const html = DOMPurify.sanitize(raw);
  return <div className="md-body" dangerouslySetInnerHTML={{ __html: html }} />;
}

function CodeBlock({ lang, content }: { lang: string; content: string }) {
  const [copied, setCopied] = useState(false);

  const highlighted = (() => {
    try {
      if (lang && lang !== "plaintext" && hljs.getLanguage(lang)) {
        return hljs.highlight(content, { language: lang }).value;
      }
      return hljs.highlightAuto(content).value;
    } catch {
      return content
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;");
    }
  })();

  const handleCopy = () => {
    void navigator.clipboard.writeText(content).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1800);
    });
  };

  return (
    <div className="md-code-block">
      <div className="md-code-block__header">
        <span className="md-code-block__lang">{lang}</span>
        <button className="md-code-block__copy" onClick={handleCopy} title="复制代码">
          {copied ? (
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="#22c55e" strokeWidth="2.5">
              <polyline points="20 6 9 17 4 12" />
            </svg>
          ) : (
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2" />
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
            </svg>
          )}
        </button>
      </div>
      <pre className="md-code-block__pre">
        <code dangerouslySetInnerHTML={{ __html: highlighted }} />
      </pre>
    </div>
  );
}

export default function MarkdownRenderer({ content }: { content: string }) {
  const segments = parseSegments(content);
  return (
    <div className="md-renderer">
      {segments.map((seg, i) =>
        seg.type === "code" ? (
          <CodeBlock key={i} lang={seg.lang ?? "text"} content={seg.content} />
        ) : (
          <MarkdownBlock key={i} content={seg.content} />
        )
      )}
    </div>
  );
}
