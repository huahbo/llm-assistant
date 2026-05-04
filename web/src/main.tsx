import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { RuntimeProvider } from "./contexts/RuntimeContext";
import { VaultProvider } from "./contexts/VaultContext";
import "./styles.css";

const root = ReactDOM.createRoot(document.getElementById("root") as HTMLElement);

root.render(
  <React.StrictMode>
    <RuntimeProvider>
      <VaultProvider>
        <App />
      </VaultProvider>
    </RuntimeProvider>
  </React.StrictMode>,
);
