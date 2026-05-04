# H7 App.tsx 拆分重构计划（架构设计稿）

> 创建日期：2026-04-30
> 设计模型：Opus 4.7（架构）
> 执行模型：Sonnet 4.6（增量提取）
> 前置条件：`docs/dev-status.md` 基线 233 通过 + typecheck 零错 + write/edit_wiki 自动化测试已覆盖（见 §0.1）
> 适用文件：`web/src/App.tsx`（13858 行 / 560 KB / 单组件 9500 行）

---

## 0. 摘要（先看）

App.tsx 单文件 13858 行、内嵌主组件 9500 行、235+ useState、357 hooks。已出现 Babel deopt 警告，三方协作 merge 冲突频繁，模块化测试不可能。

本计划按 **"Context + 模块组件"** 路线，**不引入新依赖**（不上 Zustand/Redux/Router），**纯 React 内置能力**完成拆分。

最终目标：
- App.tsx 收敛到 **< 500 行**（Provider 编排 + 模块路由）
- 10 个模块独立到 `web/src/modules/<name>/`
- 跨模块状态归入 5 个 Context
- **行为零变化**（refactor != rewrite）

预计执行时间：Sonnet 4.6 增量执行约 **15-20 个 commit**，每个 commit 可独立回滚。

### 0.1 关于 write_wiki / edit_wiki E2E 验证

**自动化测试已覆盖**（`src-tauri/src/state.rs`）：

| 测试名 | 路径 |
|---|---|
| `approve_agent_write_full_write_creates_file` | 全写审批通过 → 文件落盘 ✅ |
| `reject_agent_write_does_not_create_file` | 拒绝 → 文件不创建 ✅ |
| `approve_agent_write_patch_replaces_content` | patch 审批通过 → 内容替换 ✅ |
| `approve_agent_write_patch_fails_when_old_str_not_found` | old_str 缺失 → 失败回滚 ✅ |

**用户决定**：跳过手测，以自动化覆盖为准。本文件 §0.1 即为该决策记录。

---

## 1. 目标 / 非目标

### 1.1 目标

- 把 App.tsx 从 13858 行降到 **< 500 行**
- 每个 module 文件 **< 1500 行**（Agent Studio 因复杂可放宽到 < 2000 行）
- 状态分类清晰：全局（Context）/ 模块（组件本地）/ 派生（useMemo）
- 行为对照原版 100% 一致（pixel-level 视觉对照、操作链路对照）
- 每个 commit 独立 typecheck / test / 视觉对照通过
- Sonnet 4.6 可按本计划独立执行，无需重新对齐架构

### 1.2 非目标（本轮不做）

- ❌ 新增功能或修 bug
- ❌ 引入状态管理库（Zustand / Redux / Recoil / Jotai）
- ❌ 引入路由库（React Router / TanStack Router），保留 `activeModule` 字符串路由
- ❌ 拆分 styles.css（127 KB 单文件，下一轮再处理）
- ❌ 拆分 tauri-client.ts（63 KB 单文件，下一轮再处理）
- ❌ 大规模改 props 接口（保留 SDK 风格不变）

### 1.3 验收标准（DoD）

- [ ] `npm run typecheck` 零错误
- [ ] `cd src-tauri; cargo test` **233+ 通过**（不应减少）
- [ ] `npm --workspace web run test` 全部通过
- [ ] `wc -l web/src/App.tsx` < 500 行
- [ ] 10 个模块文件均存在于 `modules/` 下
- [ ] 5 个 Context 文件均存在于 `contexts/` 下
- [ ] Windows 端 `npm --workspace web run dev` 启动正常
- [ ] 手测对照（用户在 Windows 端逐模块切换，行为一致）

---

## 2. 现状诊断

### 2.1 量化指标

| 维度 | 当前值 | 目标值 |
|---|---|---|
| App.tsx 行数 | 13858 | < 500 |
| App.tsx 文件大小 | 560 KB | < 30 KB |
| 主组件行数 | ~9500（line 2952-12424） | < 200 |
| 主组件 useState | 236 | < 20（仅顶层 Provider 状态） |
| 总 hooks | 357 | 分散到各模块 |
| 已内嵌子组件 | 4（QueuePanel / ResearchDialog / ResearchPanel / SearchConfigPanel） | 全部移出 |

### 2.2 模块边界（line 范围已实测）

```
8452-8930    inbox       (~479 行)
8931-9227    wiki        (~296 行)
9228-9689    ask         (~461 行)
9690-10051   lint        (~361 行)
10052-10645  graph       (~593 行)
10646-10971  settings    (~325 行)
10972-11121  operations  (~149 行)
11122-11135  research    (~13 行 — 仅渲染 ResearchPanel)
11136-12424  agent       (~1288 行 — 最复杂)
12425-13858  4 个内嵌子组件 + main App return 收尾
```

### 2.3 状态分布画像

主组件 236 个 useState，按命名前缀粗分：

| 前缀 | 数量（估） | 归属 |
|---|---|---|
| `agent*` | ~50 | Agent Studio 模块 |
| `lint*` | ~15 | Lint 模块 |
| `ask*` | ~15 | Ask 模块 |
| `wiki*` / `pages` | ~10 | Wiki 模块 |
| `ingest*` | ~10 | Inbox 模块 |
| `graph*` | ~10 | Graph 模块 |
| `research*` | ~8 | Research 模块 |
| `vault*` / `recent*` | ~5 | **全局** |
| `mode*` / `activeModule` | ~3 | **全局** |
| `llm*` / `provider*` | ~10 | Settings + 跨模块只读 |
| `shellPolicy*` | ~5 | 跨 Agent + Settings |
| `statusMessage` / `agentStatusMessage` | ~3 | 跨模块通知 |
| 其他 | ~80 | 多数为模块本地、少量待分类 |

**关键发现**：~25 个状态是真正全局/跨模块的，**剩余 ~210 个都是模块本地状态**，应跟随模块下沉。

---

## 3. 目标架构

### 3.1 目录结构

```
web/src/
├── App.tsx                          # ~400 行：Provider 编排 + 模块路由
├── main.tsx                         # 不动
├── types.ts                         # 不动（全局类型）
├── tauri-client.ts                  # 不动（本轮不拆）
├── templates.ts                     # 不动
├── lint-utils.ts                    # 不动
├── app-formatters.ts                # 不动
├── env.d.ts / assets.d.ts           # 不动
│
├── contexts/                        # 全局共享状态
│   ├── RuntimeContext.tsx           # isTauri、dev mode 等只读环境
│   ├── VaultContext.tsx             # vaultPath、recentVaultPaths、initVault
│   ├── ModeContext.tsx              # activeModule + navigation
│   ├── ShellPolicyContext.tsx       # 5 维 shell 策略 + profile 切换
│   └── ToastContext.tsx             # statusMessage 统一收口
│
├── modules/                         # 10 个模块
│   ├── inbox/
│   │   ├── InboxModule.tsx
│   │   └── (按需子组件)
│   ├── wiki/
│   │   ├── WikiModule.tsx
│   │   └── (按需子组件)
│   ├── ask/
│   │   └── AskModule.tsx
│   ├── lint/
│   │   ├── LintModule.tsx
│   │   ├── LintPatchPanel.tsx
│   │   └── SearchConfigPanel.tsx    # 已存在内嵌组件移出
│   ├── graph/
│   │   └── GraphModule.tsx
│   ├── research/
│   │   ├── ResearchModule.tsx
│   │   ├── ResearchPanel.tsx        # 已存在内嵌组件移出
│   │   └── ResearchDialog.tsx       # 已存在内嵌组件移出
│   ├── operations/
│   │   ├── OperationsModule.tsx
│   │   └── QueuePanel.tsx           # 已存在内嵌组件移出
│   ├── settings/
│   │   ├── SettingsModule.tsx
│   │   ├── LlmProviderPanel.tsx
│   │   └── ShellPolicySettingsPanel.tsx
│   └── agent/
│       ├── AgentStudio.tsx          # 主入口
│       ├── AgentChatPane.tsx        # 左侧聊天区
│       ├── AgentToolsPane.tsx       # 右侧工具区（含 Shell 抽屉）
│       ├── AgentReviewPane.tsx      # 草稿审阅
│       ├── AgentRunHistory.tsx      # 历史 runs 卡片条
│       ├── AgentMemoryPanel.tsx     # 记忆 CRUD
│       ├── AgentSkillsPanel.tsx     # 技能 CRUD
│       ├── AgentShellPolicyPanel.tsx# Agent 工具页档位（轻量版）
│       └── hooks/
│           ├── useAgentRuns.ts      # 拢相关 useState + useEffect
│           ├── useAgentDrafts.ts
│           ├── useAgentEvents.ts
│           ├── useAgentMemories.ts
│           └── useAgentSkills.ts
│
└── shared/                          # 跨模块通用
    ├── components/
    │   ├── Sidebar.tsx
    │   ├── ModuleHeader.tsx
    │   └── (按需)
    └── hooks/
        ├── useTauriRuntime.ts
        └── (按需)
```

### 3.2 Context 设计契约

**核心原则**：Context 只放**真正跨 2+ 模块**的状态。模块独享的状态绝不进 Context。

#### RuntimeContext

```ts
type RuntimeValue = {
  isTauri: boolean;          // 是否在 Tauri 运行时
  // 派生自 isTauriRuntime() 的纯只读值，不会变化
};
```

**消费者**：所有模块（用于禁用按钮）

#### VaultContext

```ts
type VaultValue = {
  vaultPath: string;
  recentVaultPaths: string[];
  setVaultPath: (path: string) => void;
  pushRecentVaultPath: (path: string) => void;
  initVault: () => Promise<void>;          // 初始化当前 vault
};
```

**消费者**：所有模块（基本都要读 vault）

#### ModeContext

```ts
type ModeValue = {
  activeModule: ModuleId;
  navigateTo: (id: ModuleId) => void;      // 含 queue/stats → operations 兼容（已移除，但保留接口防回归）
};
```

**消费者**：Sidebar、所有模块的"返回首页"按钮、个别模块的内部跳转

#### ShellPolicyContext

```ts
type ShellPolicyValue = {
  config: ShellPolicyConfig | null;
  saving: boolean;
  dirty: boolean;
  reload: () => Promise<void>;
  save: () => Promise<void>;
  applyProfile: (profile: ShellPolicyProfileKey) => void;
  applyAndSaveProfile: (profile: ShellPolicyProfileKey) => Promise<void>;
  setField: (field: keyof ShellPolicyConfig, value: ShellPolicyDecision) => void;
};
```

**消费者**：Settings（完整 UI）、Agent（档位按钮 + tooltip）

#### ToastContext

```ts
type ToastValue = {
  // 统一原 statusMessage / agentStatusMessage 为单通道
  // 默认时长 3s，关键提示可锁定
  push: (message: string, options?: { sticky?: boolean }) => void;
  message: string;
};
```

**消费者**：全部模块（替代当前 ~3 个分散的 status state）

### 3.3 状态归类规则（给 Sonnet 4.6 的判定标准）

接到一个 useState，按以下顺序判断归属：

```
1. 是否被 ≥ 2 个模块读？
   是 → Context（参考 §3.2）
   否 → 进入 2

2. 是否被 Sidebar / ModuleHeader 等顶层组件读？
   是 → Context（VaultContext 或 ModeContext）
   否 → 进入 3

3. 是否纯派生自其他状态？
   是 → 改成 useMemo / 删除
   否 → 进入 4

4. 是否仅在某个模块内使用？
   是 → 移到该模块组件 useState
   否 → 重新走 1（不应到这里，若到则需人工判定）
```

### 3.4 Handler 函数归属规则

```
1. handler 操作的是全局状态？（vault / activeModule / shellPolicy / toast）
   是 → 放在对应 Context Provider 的实现里
   否 → 进入 2

2. handler 仅被一个模块调用？
   是 → 移到该模块组件
   否 → 进入 3

3. handler 被多个模块调用？
   是 → 提取到 shared/hooks/use<X>.ts
   否 → 走 1
```

---

## 4. 阶段化执行计划

> **重要**：每个阶段每个步骤独立 commit，独立 typecheck + 测试。
> Sonnet 4.6 不要把多个阶段塞进一个 commit。

### Phase 0 — 准备：4 个内嵌组件外移（低风险）

**预估**：30 分钟，4 个 commit

#### Step 0.1 — QueuePanel → modules/operations/

```
- 创建 web/src/modules/operations/QueuePanel.tsx
- 把 App.tsx line 12425 的 function QueuePanel(...) 整个块剪过去
- 加 export default function QueuePanel(...)
- App.tsx 顶部 import QueuePanel from "./modules/operations/QueuePanel"
- 删除 App.tsx 内嵌定义
```

验证：
```powershell
cd E:\llm-wiki\web
npm run typecheck
```

Commit：`refactor(web): 提取 QueuePanel 到 modules/operations/`

#### Step 0.2 — ResearchPanel → modules/research/ResearchPanel.tsx

同模板，line 13073 的 function ResearchPanel。

#### Step 0.3 — ResearchDialog → modules/research/ResearchDialog.tsx

同模板，line 12512 的 function ResearchDialog。

#### Step 0.4 — SearchConfigPanel → modules/lint/SearchConfigPanel.tsx

同模板，line 13710 的 function SearchConfigPanel。

**Phase 0 收尾**：App.tsx 应剩 ~12420 行（少 ~1430 行，其中 ~1430 是 4 个组件搬走）。

---

### Phase 1 — Context 框架建立（中风险）

**预估**：2 小时，5 个 commit

**核心原则**：每个 Context 单独提交，App.tsx 仍**保留所有 useState 不动**。Context 在这个阶段只是"新增的数据通道"，旧逻辑继续工作。模块阶段（Phase 2）才会真正切换数据源。

#### Step 1.1 — RuntimeContext

```tsx
// web/src/contexts/RuntimeContext.tsx
import { createContext, useContext, useMemo, type ReactNode } from "react";
import { isTauriRuntime } from "../tauri-client";

type RuntimeValue = {
  isTauri: boolean;
};

const RuntimeContext = createContext<RuntimeValue | null>(null);

export function RuntimeProvider({ children }: { children: ReactNode }) {
  const value = useMemo<RuntimeValue>(() => ({ isTauri: isTauriRuntime() }), []);
  return <RuntimeContext.Provider value={value}>{children}</RuntimeContext.Provider>;
}

export function useRuntime() {
  const value = useContext(RuntimeContext);
  if (!value) throw new Error("useRuntime 必须在 <RuntimeProvider> 内使用");
  return value;
}
```

App.tsx 改动：
- 顶部 `import { RuntimeProvider } from "./contexts/RuntimeContext";`
- main return 外层包 `<RuntimeProvider>...</RuntimeProvider>`

验证：typecheck 通过即可（无消费者，只是建立通道）。

Commit：`refactor(web): 建立 RuntimeContext`

#### Step 1.2 — VaultContext

把 App.tsx 中的 `vaultPath / recentVaultPaths / setVaultPath` 等 5 个状态**复制**到 VaultContext（注意：复制不删除）。Context 的状态独立维护一份。Phase 2 模块拆分时再切换数据源到 Context。

**或者更激进**：直接把 vault 状态移到 VaultContext，App.tsx 改用 `useVault()`。

**推荐做法**：复制方式更安全，Phase 2 切换时不用回头改两份。但带来短期数据冗余。Sonnet 4.6 自行判断（默认采用激进方式：直接移走，App.tsx 改用 hook）。

Commit：`refactor(web): 抽取 VaultContext`

#### Step 1.3 — ModeContext

同 1.2，处理 `activeModule + navigation handler`。

Commit：`refactor(web): 抽取 ModeContext`

#### Step 1.4 — ShellPolicyContext

把 App.tsx 中：
- `agentShellPolicyConfig / agentShellPolicySaving / agentShellPolicyDirty`
- `handleSaveShellPolicy / applyShellPolicyProfile / handleApplyAndSaveShellPolicyProfile / handleReloadShellPolicy / handleChangeShellPolicyDecision`

整体迁到 ShellPolicyContext。App.tsx 改用 `useShellPolicy()`。

Commit：`refactor(web): 抽取 ShellPolicyContext，统一 Agent + Settings 数据源`

#### Step 1.5 — ToastContext

把 `statusMessage / agentStatusMessage / setStatusMessage / setAgentStatusMessage` 统一为 `useToast().push(...)`。

**注意**：原代码可能有"Agent 区单独显示"和"全局区显示"两种通道。本次合并为单通道，UI 上原显示位置保持，从 ToastContext 取值即可。

Commit：`refactor(web): 抽取 ToastContext，统一 status message 通道`

**Phase 1 收尾**：App.tsx 仍 ~12420 行，但顶层多了 5 个 Provider 包裹，全局状态从 App.tsx 移走 ~25 个，余 ~210 个 useState。

---

### Phase 2 — 模块提取（按风险递增）

**预估**：每个模块 1-2 小时，9 个 commit。

#### 通用模板（所有模块都按这个流程）

对每个模块 `<X>`：

1. **创建文件**：`web/src/modules/<x>/<X>Module.tsx`
2. **复制 JSX**：把 App.tsx 中 `{activeModule === "<x>" && (...)}` 内的 JSX 整体剪过去，作为 `<X>Module` 组件 return 内容
3. **识别状态**：列出该 JSX 引用的所有 useState 名称，按 §3.3 规则判定归属：
   - 仅本模块用 → 移到 `<X>Module` 组件内部 useState
   - 跨模块用 → 已经在 Phase 1 进 Context，改用 `useXxx()` hook
4. **识别 handlers**：同上，按 §3.4 规则判定
5. **识别 useEffect**：把仅服务该模块的 useEffect 移过去；跨模块的留在 App.tsx
6. **识别衍生类型**：模块独享类型（如 `AgentReviewTab`）移到 `modules/<x>/types.ts`
7. **接线**：
   - App.tsx import `<X>Module`
   - `{activeModule === "<x>" && <XModule {...neededProps} />}`
   - 所需 props 应非常少（理想为 0，全靠 Context）
8. **验证**：
   ```powershell
   cd E:\llm-wiki\web
   npm run typecheck
   npm --workspace web run test
   ```
9. **视觉对照**：Windows 端 `npm run dev`，切换到该模块，检查行为对照原版（关键交互点 3-5 个）
10. **Commit**：`refactor(web): 提取 <X> 模块到 modules/<x>/`

#### Step 2.1 — Settings 模块

```
源 line: 10646-10971 (~325 行)
目标: modules/settings/SettingsModule.tsx
+ modules/settings/LlmProviderPanel.tsx (LLM Provider 卡片单独提)
+ modules/settings/ShellPolicySettingsPanel.tsx (5 维 + 2 维 = 7 维下拉控件)
状态: ~10 个（llm* / mode* / shellPolicy* 已进 Context）
风险: 低 — 主要是表单
```

#### Step 2.2 — Operations 模块

```
源 line: 10972-11121 (~149 行)
目标: modules/operations/OperationsModule.tsx
依赖: Phase 0.1 已提取的 QueuePanel
状态: ~5 个（operationsTab / ingestQueue / vaultStats）
风险: 低 — 已部分独立
```

#### Step 2.3 — Inbox 模块

```
源 line: 8452-8930 (~479 行)
目标: modules/inbox/InboxModule.tsx
状态: ~10 个 ingest* 系列
风险: 低 — 表单 + 文件选择
```

#### Step 2.4 — Lint 模块

```
源 line: 9690-10051 (~361 行)
目标: modules/lint/LintModule.tsx
+ modules/lint/LintPatchPanel.tsx (lint patch preview/apply 卡片单独提)
依赖: Phase 0.4 已提取的 SearchConfigPanel
状态: ~15 个 lint* 系列
风险: 中 — patch 流程逻辑较绕
```

#### Step 2.5 — Wiki 模块

```
源 line: 8931-9227 (~296 行)
目标: modules/wiki/WikiModule.tsx
状态: ~10 个 pages / wiki* 系列
风险: 中 — 涉及页面历史回滚逻辑
```

#### Step 2.6 — Ask 模块

```
源 line: 9228-9689 (~461 行)
目标: modules/ask/AskModule.tsx
状态: ~15 个 ask* 系列（含 askSessions / askHistoryKeyword 等）
风险: 中 — 含会话切换 + debug 面板
```

#### Step 2.7 — Graph 模块

```
源 line: 10052-10645 (~593 行)
目标: modules/graph/GraphModule.tsx
状态: ~10 个 graph* 系列
风险: 中 — Canvas 渲染依赖 useEffect / useRef，提取时注意 effect 顺序
```

#### Step 2.8 — Research 模块

```
源 line: 11122-11135 (~13 行)
目标: modules/research/ResearchModule.tsx
依赖: Phase 0.2/0.3 已提取的 ResearchPanel + ResearchDialog
状态: ~3 个
风险: 低 — 已基本独立
```

#### Step 2.9 — Agent Studio 模块（最后做，最复杂）

**这是本计划的高风险高价值步骤，单独展开。**

```
源 line: 11136-12424 (~1288 行)
目标: modules/agent/ 多个文件
状态: ~50 个 agent* 系列
风险: 高 — 状态最多、逻辑最复杂
```

**子拆分策略**（建议分 5 个 commit，不要一次完成）：

##### Step 2.9.1 — 抽取 Agent 模块骨架 + AgentMemoryPanel

```
- 创建 modules/agent/AgentStudio.tsx（先把整块 JSX 剪过去）
- 把 agent_memory* 5 个 state + 相关 handler 抽到 modules/agent/AgentMemoryPanel.tsx
- AgentStudio import AgentMemoryPanel
- 验证 Memory CRUD 功能
Commit: refactor(web): 抽取 Agent Studio 骨架 + AgentMemoryPanel
```

##### Step 2.9.2 — AgentSkillsPanel

```
- 把 agent_skill* 系列状态 + handler 抽到 AgentSkillsPanel.tsx
Commit: refactor(web): 抽取 AgentSkillsPanel
```

##### Step 2.9.3 — AgentRunHistory

```
- 把历史 runs 卡片条 + 归档/恢复 handler 抽到 AgentRunHistory.tsx
Commit: refactor(web): 抽取 AgentRunHistory（含归档管理）
```

##### Step 2.9.4 — AgentToolsPane（Shell 抽屉 + 档位按钮）

```
- 把 Shell 终端 + 工具按钮 + 档位预设抽到 AgentToolsPane.tsx
- 共用 ShellPolicyContext（Phase 1.4 已建立）
Commit: refactor(web): 抽取 AgentToolsPane（Shell 抽屉 + 档位）
```

##### Step 2.9.5 — AgentChatPane + AgentReviewPane

```
- 把聊天/任务输入区抽到 AgentChatPane.tsx
- 把草稿审阅 / diff / citations tab 抽到 AgentReviewPane.tsx
- AgentStudio.tsx 收敛为容器：<AgentChatPane /> <AgentReviewPane /> <AgentToolsPane />
Commit: refactor(web): 收敛 AgentStudio 容器，拆分 ChatPane + ReviewPane
```

##### Step 2.9.6（可选）— hooks 抽取

```
- 把 useAgentRuns / useAgentDrafts / useAgentEvents 等抽到 modules/agent/hooks/
- 这一步是优化，不影响行为
Commit: refactor(web): 把 Agent 相关 useEffect 拢到独立 hooks
```

**Phase 2 收尾**：App.tsx 应只剩：
- imports
- App() 函数
- 顶层 Provider 编排
- Sidebar
- module router（10 行 switch）
- 总计 < 500 行

---

### Phase 3 — 收口与验证（必做）

**预估**：1 小时

#### Step 3.1 — App.tsx 减肥验证

```powershell
wc -l web/src/App.tsx           # 应 < 500
```

如果 > 500，回查是否有遗漏未抽取的状态/handler/useEffect。

#### Step 3.2 — 完整测试套件

```powershell
cd E:\llm-wiki\web
npm run typecheck                # 必须 0 错误
npm --workspace web run test    # 必须全部通过

cd ..\src-tauri
cargo test                       # 必须 ≥ 233 通过
```

#### Step 3.3 — Windows 端启动 + 全模块手测对照

```powershell
cd E:\llm-wiki\web
npm run dev
```

逐模块切换，对照重构前的行为（关键交互各取 3 点）：
- inbox: 选择文件 + 入队 + 队列状态显示
- wiki: 翻页 + 搜索 + 历史回滚
- ask: 输入查询 + 切换 session + ask_first
- lint: 跑 lint + patch preview + apply
- graph: 渲染 + 点击节点 + 子图筛选
- settings: 切 mode + 改 Provider + 改 Shell 策略 5 维 + 7 维（含 network/script）
- operations: 切 tab（队列/统计）
- research: 启动 research + dialog 显示
- agent: 任务模式 + 草稿模式 + 历史 runs + Memory CRUD + Skills CRUD + 写入审批

每个模块对照通过后在本文件 §5 打勾。

---

## 5. 进度跟踪（Sonnet 4.6 执行时填写）

### Phase 0
- [x] 0.1 QueuePanel
- [x] 0.2 ResearchPanel
- [x] 0.3 ResearchDialog
- [x] 0.4 SearchConfigPanel

### Phase 1
- [x] 1.1 RuntimeContext
- [x] 1.2 VaultContext
- [x] 1.3 ModeContext
- [x] 1.4 ShellPolicyContext
- [x] 1.5 ToastContext

### Phase 2
- [x] 2.1 Settings
- [x] 2.2 Operations
- [ ] 2.3 Inbox
- [ ] 2.4 Lint
- [ ] 2.5 Wiki
- [ ] 2.6 Ask
- [ ] 2.7 Graph
- [ ] 2.8 Research
- [ ] 2.9.1 Agent 骨架 + Memory
- [ ] 2.9.2 Agent Skills
- [ ] 2.9.3 Agent RunHistory
- [ ] 2.9.4 Agent Tools
- [ ] 2.9.5 Agent Chat + Review
- [ ] 2.9.6 Agent hooks（可选）

### Phase 3
- [ ] 3.1 App.tsx < 500 行
- [ ] 3.2 测试套件全绿
- [ ] 3.3 Windows 端手测全模块对照

---

## 6. 风险与回滚

### 6.1 主要风险

| 风险 | 触发 | 缓解 |
|---|---|---|
| 状态归类错误 | Sonnet 4.6 把模块本地状态误升 Context | 每个 commit 独立测试 + 视觉对照 |
| useEffect 顺序错乱 | 提取时改变 effect 触发顺序 | 同模块的 effect 一起搬，不混搬 |
| 跨模块隐性依赖 | 状态 A 在模块 X，但模块 Y 也读 | 通过 grep 全文搜索状态名，找出所有引用 |
| TypeScript 类型 narrowing 丢失 | 拆分后 inferred type 变 unknown | 显式标注关键 props 类型 |
| Babel deopt 不缓解 | App.tsx 仍很大但 Babel 处理子文件慢 | 实测，若问题，单独优化 |
| Agent Studio 拆错 | 状态最多，最复杂 | 分 5-6 个 commit，逐个验证 |

### 6.2 回滚策略

- **单 commit 回滚**：发现回归立即 `git revert <hash>`
- **阶段回滚**：Phase 2 某模块挂了，只回滚那个模块的 commit
- **全量回滚**：极端情况下回到 Phase 0 之前（commit `5d6355d` 是 P1 完成基线，再前一个是 `aa8f9e1` 文档）

### 6.3 紧急止损

- 若 Phase 2 某模块卡住超过 2 小时未通过验证 → 暂停，更新 dev-status.md，留给下一轮人工评估
- 若 typecheck 有超过 20 个错误 → 直接 revert 到上个 commit，重新设计该模块的拆法
- 若发现 Context 设计错误（如状态泄漏到不该有的模块） → revert 该 Context commit，重新设计

---

## 7. 给 Sonnet 4.6 的执行清单

### 7.1 启动前必读

按顺序读：
1. **本文件 §0-§3**（理解架构）
2. **本文件 §4**（执行步骤）
3. `agents.md` §11（中文注释要求）、§16（多 Agent 交接）
4. `docs/dev-status.md`（确认基线 233 + typecheck 零错）
5. `docs/测试与验证规范.md`（测试标准）
6. `docs/贡献规范.md`（commit 风格）

### 7.2 执行约束

- ❌ **不要**一次提交多个 step
- ❌ **不要**跳过 typecheck
- ❌ **不要**删除测试代码
- ❌ **不要**改任何后端代码（src-tauri/）
- ❌ **不要**引入新 npm 依赖
- ✅ 每个 step 独立 commit
- ✅ commit message 严格按 §4 模板
- ✅ 中文注释（新增组件用中文说明用途）
- ✅ 提取过程中保持原有 useState 名称不变（重命名是另一个轮次的工作）

### 7.3 异常处理

遇到以下情况立即停止，更新 `docs/dev-status.md` 留接力：

- typecheck 错误 > 20 条
- cargo test 出现新失败（不应发生，除非误改了后端）
- 某模块视觉对照不一致且 30 分钟内定位不到原因
- 发现状态归类规则（§3.3）覆盖不到的边界情况

### 7.4 进度记录

每完成一个 step：
1. 在本文件 §5 对应项打 `[x]`
2. 在 `docs/实施过程记录.md` 顶部添加一条（按现有格式）：
   ```
   ## 2026-XX-XX — H7 Step <X.Y> <名称>（Sonnet 4.6）
   - 改动文件：...
   - 验证：typecheck ✅ / cargo test ✅ / 视觉对照 ✅
   - 行数变化：App.tsx XXXXX → YYYYY
   ```
3. commit 后立即开始下一个 step

每完成一个 Phase：
- 更新 `docs/dev-status.md` 基线数字 + 最新提交表
- 更新 `docs/交接状态卡.md` 当前状态

### 7.5 完成判定

执行完 §5 全部勾选后，按 §1.3 DoD 复核。复核通过 → 在本文件末尾加一行：

```
## 完成
- 完成日期：2026-XX-XX
- 最终基线：cargo test XXX 通过 / typecheck 零错 / App.tsx XXX 行
- 最后 commit：<hash>
```

并通知用户："H7 重构完成，App.tsx 从 13858 行降至 XXX 行，N 个模块独立。请人工验证。"

---

## 8. 附录：关键代码片段模板

### 8.1 Module 组件骨架

```tsx
// web/src/modules/<x>/<X>Module.tsx
import { useEffect, useState } from "react";
import { useVault } from "../../contexts/VaultContext";
import { useRuntime } from "../../contexts/RuntimeContext";
import { useToast } from "../../contexts/ToastContext";
// ...其他 imports

export default function <X>Module() {
  const { vaultPath } = useVault();
  const { isTauri } = useRuntime();
  const { push } = useToast();

  // 模块本地状态（搬过来的 useState）
  const [foo, setFoo] = useState<...>(...);

  // 模块本地 effect
  useEffect(() => {
    // ...
  }, [...]);

  // 模块本地 handler
  const handleSomething = () => { /* ... */ };

  return (
    <>
      {/* 把 App.tsx 中 {activeModule === "<x>" && (...)} 内的 JSX 直接剪过来 */}
    </>
  );
}
```

### 8.2 App.tsx 收敛后骨架（目标）

```tsx
// web/src/App.tsx — 重构后 < 500 行
import { useEffect } from "react";
import { RuntimeProvider } from "./contexts/RuntimeContext";
import { VaultProvider } from "./contexts/VaultContext";
import { ModeProvider, useMode } from "./contexts/ModeContext";
import { ShellPolicyProvider } from "./contexts/ShellPolicyContext";
import { ToastProvider } from "./contexts/ToastContext";
import Sidebar from "./shared/components/Sidebar";
import InboxModule from "./modules/inbox/InboxModule";
import WikiModule from "./modules/wiki/WikiModule";
import AskModule from "./modules/ask/AskModule";
import LintModule from "./modules/lint/LintModule";
import GraphModule from "./modules/graph/GraphModule";
import SettingsModule from "./modules/settings/SettingsModule";
import OperationsModule from "./modules/operations/OperationsModule";
import ResearchModule from "./modules/research/ResearchModule";
import AgentStudio from "./modules/agent/AgentStudio";

function ModuleRouter() {
  const { activeModule } = useMode();
  switch (activeModule) {
    case "inbox":      return <InboxModule />;
    case "wiki":       return <WikiModule />;
    case "ask":        return <AskModule />;
    case "lint":       return <LintModule />;
    case "graph":      return <GraphModule />;
    case "settings":   return <SettingsModule />;
    case "operations": return <OperationsModule />;
    case "research":   return <ResearchModule />;
    case "agent":      return <AgentStudio />;
    default:           return null;
  }
}

export default function App() {
  return (
    <RuntimeProvider>
      <ToastProvider>
        <VaultProvider>
          <ShellPolicyProvider>
            <ModeProvider>
              <div className="app-shell">
                <Sidebar />
                <main className="module-viewport">
                  <ModuleRouter />
                </main>
              </div>
            </ModeProvider>
          </ShellPolicyProvider>
        </VaultProvider>
      </ToastProvider>
    </RuntimeProvider>
  );
}
```

### 8.3 commit message 模板

```
refactor(web): <动作> <对象>

- 改动文件: <列表>
- 行数变化: App.tsx XXXXX → YYYYY (-NNN)
- 验证: typecheck ✅ / cargo test 233 ✅
- 视觉对照: <模块名> 关键交互 N 项一致

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>
```

---

## 9. FAQ

**Q1：为什么不引入 Zustand？**
A：本项目目前不需要全局 store，Context 已够。引入新依赖意味着所有人需要学习，而且 Context 的回退路径更稳。Zustand 留作下一轮（如 H8）的可选项。

**Q2：为什么不引入 React Router？**
A：当前 `activeModule` 是 string union，已是简化版路由。引入 router 库会带来 URL 同步、history 管理等额外复杂度，本轮目标是"减"不是"加"。

**Q3：模块之间如何通信（除了 Context）？**
A：本计划下，模块之间**不应直接通信**。如果需要（如 Lint 模块改了 Wiki 内容，Wiki 模块需感知）→ 通过 Context 中转，或通过 Tauri 命令重新拉数据。

**Q4：Agent Studio 这么大，能不能不拆这么细？**
A：可以一次只拆 2.9.1 + 2.9.4（骨架 + 工具区），其余子组件留作后续轮次。但建议至少完成到 2.9.4，因为 Shell 策略已经独立到 Context，对应 UI 也应配套独立。

**Q5：拆到一半 Sonnet 4.6 限流怎么办？**
A：本计划设计就是为这个场景。每个 commit 独立可回滚，Sonnet 4.6 重启后读 §5 进度勾选表，从下一个未完成 step 继续即可。不需要重新对齐架构。

---

## 10. 引用与依据

- 现状基线：commit `9b5e3a7`（H6-P1 完成 + 代码质量加固）
- 项目约束：`agents.md` §1-§16
- 测试规范：`docs/测试与验证规范.md`
- 贡献规范：`docs/贡献规范.md`
- 多 Agent 协议：`docs/多Agent通信与交接协议.md`

---

> **End of plan.** Sonnet 4.6 可按此独立执行，无需再回头与架构师对齐。
