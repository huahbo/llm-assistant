import { createContext, useContext, useState } from "react";
import type { ReactNode } from "react";

interface GraphBridgeValue {
  highlightedPaths: string[];
  setHighlightedPaths: (paths: string[]) => void;
  chatPrefill: string;
  setChatPrefill: (text: string) => void;
  askPrefill: string;
  setAskPrefill: (text: string) => void;
}

const GraphBridgeContext = createContext<GraphBridgeValue>({
  highlightedPaths: [],
  setHighlightedPaths: () => {},
  chatPrefill: "",
  setChatPrefill: () => {},
  askPrefill: "",
  setAskPrefill: () => {},
});

export function GraphBridgeProvider({ children }: { children: ReactNode }) {
  const [highlightedPaths, setHighlightedPaths] = useState<string[]>([]);
  const [chatPrefill, setChatPrefill] = useState("");
  const [askPrefill, setAskPrefill] = useState("");

  return (
    <GraphBridgeContext.Provider
      value={{ highlightedPaths, setHighlightedPaths, chatPrefill, setChatPrefill, askPrefill, setAskPrefill }}
    >
      {children}
    </GraphBridgeContext.Provider>
  );
}

export function useGraphBridge() {
  return useContext(GraphBridgeContext);
}

export function extractWikiPaths(text: string): string[] {
  const results: string[] = [];
  const re = /\[\[([^\]]+)\]\]/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    const p = m[1].trim();
    if (p) results.push(p);
  }
  return [...new Set(results)];
}
