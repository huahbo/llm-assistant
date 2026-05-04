import {
  createContext,
  useContext,
  useMemo,
  useState,
  type Dispatch,
  type ReactNode,
  type SetStateAction,
} from "react";

type ToastValue = {
  statusMessage: string;
  agentStatusMessage: string;
  setStatusMessage: Dispatch<SetStateAction<string>>;
  setAgentStatusMessage: Dispatch<SetStateAction<string>>;
};

const ToastContext = createContext<ToastValue | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [statusMessage, setStatusMessage] = useState("");
  const [agentStatusMessage, setAgentStatusMessage] = useState("");

  const value = useMemo<ToastValue>(
    () => ({
      statusMessage,
      agentStatusMessage,
      setStatusMessage,
      setAgentStatusMessage,
    }),
    [statusMessage, agentStatusMessage],
  );

  return <ToastContext.Provider value={value}>{children}</ToastContext.Provider>;
}

export function useToast() {
  const value = useContext(ToastContext);
  if (!value) throw new Error("useToast 必须在 <ToastProvider> 内使用");
  return value;
}
