import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { RuntimeProvider } from "./contexts/RuntimeContext";
import { VaultProvider } from "./contexts/VaultContext";
import { ModeProvider } from "./contexts/ModeContext";
import { ShellPolicyProvider } from "./contexts/ShellPolicyContext";
import "./styles.css";

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);

root.render(
  <React.StrictMode>
    <RuntimeProvider>
      <VaultProvider>
        <ModeProvider>
          <ShellPolicyProvider>
            <App />
          </ShellPolicyProvider>
        </ModeProvider>
      </VaultProvider>
    </RuntimeProvider>
  </React.StrictMode>,
);
