import type { ReactNode } from "react";

type AgentStudioProps = {
  statusMessage: string;
  statusTone: string;
  debugPanelOpen: boolean;
  children: ReactNode;
};

export default function AgentStudio({
  statusMessage,
  statusTone,
  debugPanelOpen,
  children,
}: AgentStudioProps) {
  return (
    <>
      <div className="module-header">
        <h1 className="module-header__title">Agent Studio</h1>
        <p className="module-header__sub">
          左侧对话驱动，右侧草稿预览与审批写盘
        </p>
      </div>
      <section className={`panel agent-studio agent-studio--b2${debugPanelOpen ? " agent-studio--debug-open" : ""}`}>
        {statusMessage ? (
          <p className={`agent-studio__status agent-studio__status--${statusTone}`}>
            {statusMessage}
          </p>
        ) : null}
        {children}
      </section>
    </>
  );
}
