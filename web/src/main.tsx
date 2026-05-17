import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { RuntimeProvider } from "./contexts/RuntimeContext";
import { VaultProvider } from "./contexts/VaultContext";
import { ModeProvider } from "./contexts/ModeContext";
import { ShellPolicyProvider } from "./contexts/ShellPolicyContext";
import { ToastProvider } from "./contexts/ToastContext";
import { GraphBridgeProvider } from "./contexts/GraphBridgeContext";
import { openExternalUrl } from "./tauri-client/base";
import "./styles.css";
import "./modules/ask/ask.css";
import "./modules/lint/lint.css";
import "./modules/wiki/wiki.css";
import "./modules/settings/settings.css";
import "./modules/graph/graph.css";
import "./modules/agent/agent.css";
import "./modules/chat/chat.css";
import "./modules/palette/palette.css";

// 拦截所有外部 http(s) 链接，防止 Tauri WebView 直接导航替换 App 界面
document.addEventListener(
  "click",
  (e: MouseEvent) => {
    const anchor = e.composedPath().find(
      (el) => el instanceof HTMLAnchorElement,
    ) as HTMLAnchorElement | undefined;
    if (!anchor) return;
    const href = anchor.getAttribute("href") ?? "";
    if (/^https?:\/\//.test(href)) {
      e.preventDefault();
      void openExternalUrl(href);
    }
  },
  true, // capture phase — 在 React 事件冒泡之前拦截
);

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);

root.render(
  <React.StrictMode>
    <RuntimeProvider>
      <VaultProvider>
        <ModeProvider>
          <ShellPolicyProvider>
            <ToastProvider>
              <GraphBridgeProvider>
                <App />
              </GraphBridgeProvider>
            </ToastProvider>
          </ShellPolicyProvider>
        </ModeProvider>
      </VaultProvider>
    </RuntimeProvider>
  </React.StrictMode>,
);
