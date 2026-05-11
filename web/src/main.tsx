import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { RuntimeProvider } from "./contexts/RuntimeContext";
import { VaultProvider } from "./contexts/VaultContext";
import { ModeProvider } from "./contexts/ModeContext";
import { ShellPolicyProvider } from "./contexts/ShellPolicyContext";
import { ToastProvider } from "./contexts/ToastContext";
import { GraphBridgeProvider } from "./contexts/GraphBridgeContext";
import "./styles.css";
import "./modules/ask/ask.css";
import "./modules/lint/lint.css";
import "./modules/wiki/wiki.css";
import "./modules/settings/settings.css";
import "./modules/graph/graph.css";
import "./modules/agent/agent.css";
import "./modules/chat/chat.css";

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
