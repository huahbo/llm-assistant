# H15 优化计划 — 2026-05-13

## 已完成（无需重做）
| 项目 | 状态 | 说明 |
|------|------|------|
| tauri-client.ts 领域拆分 | ✅ | 已拆为 chat/wiki/agent/shell/search/config 等子文件 |
| Shell 审批票据缓存 | ✅ | agent_policy.rs TicketCache + ToolCallCard "记住" checkbox |
| Shell 审计落库 | ✅ | db.rs shell_audit_events + AgentToolsPane 展示 |
| web_search 搜索源标签 | ✅ | ToolCallCard `via {source}` badge + extractSearchSource() |
| styles.css 模块拆分 | ✅ | 各模块已有独立 CSS 文件，styles.css 仅保留全局/壳层样式 |

---

## 待实施（今日目标）

### T1 — 暗黑主题（Dark Mode）🟠 高优先
**价值**：CSS 变量体系完备，切换成本极低；用户最常见需求。
**方案**：
- `styles.css` 追加 `:root[data-theme="dark"]` 变量覆盖块
- `App.tsx` 读取 localStorage `"theme"` 初始化，`html` 加 `data-theme` 属性
- 标题栏增加日/月切换图标按钮（16px SVG），写入 localStorage
- 不改动任何 .tsx 业务逻辑
**验收**：typecheck 零错误；深色/浅色切换视觉正常

### T2 — Agent 循环墙钟超时 🟡 中优先
**价值**：防止 LLM 调用卡死挂起整个 Agent 任务（当前仅有迭代次数限制）。
**方案**：
- `src-tauri/src/agent_service.rs`（或 state.rs run_agent_task）最外层加
  `tokio::time::timeout(Duration::from_secs(600), ...)` — 默认 10 分钟
- 超时后返回友好错误消息给前端
**验收**：cargo test 全绿

### T3 — React ErrorBoundary 🟡 中优先
**价值**：某模块 render 崩溃不会导致整个 app 白屏。
**方案**：
- 新增 `web/src/components/ErrorBoundary.tsx`（class component）
- 在 `App.tsx` 各模块渲染处用 `<ErrorBoundary>` 包裹
**验收**：typecheck 零错误

### T4 — 模块加载骨架屏 🟢 低优先
**价值**：初始化时给用户即时视觉反馈，消除空白闪烁。
**方案**：
- 新增 `web/src/components/SkeletonPane.tsx` — 几条 shimmer 占位行
- Chat/Wiki/Ask/Agent 初始化时显示骨架屏（替换空 div）
**验收**：typecheck 零错误

---

## 架构级重构（本次不做，需专项计划）

| 项目 | 原因 |
|------|------|
| state.rs 拆分（15k 行）| 涉及所有业务逻辑，需专项 + 全面集成测试 |
| AgentStudio 拆分（2423 行）| 需 UI 运行时测试，单次 session 风险高 |
| DB 事务管理 | 需逐条审查 READ+WRITE 组合，风险高 |
| SQLite 连接池 | 需引入 r2d2/sqlx 依赖，可能破坏现有接口 |
| Wiki 智能写作补全 | 需完整 UX 设计，不属于今日范围 |
| 知识图谱自动标注 | 需 NER/LLM pipeline 设计 |

---

## 执行顺序
```
T1 暗黑主题 → T2 Agent 超时 → T3 ErrorBoundary → T4 骨架屏
```
每项完成后 typecheck + cargo test + commit。最后统一 push。
