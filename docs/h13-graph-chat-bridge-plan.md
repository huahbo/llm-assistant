# H13：Chat ↔ Graph 双向联动实施计划

> 状态：待实施 | 优先级：高（纯前端，零后端风险）
> 依赖：H9 Chat UI ✅ | H7 Graph 模块 ✅

---

## 1. 目标

让 Chat 与 Graph 两个模块彼此感知，形成双向导航闭环：

| 方向 | 触发 | 效果 |
|------|------|------|
| Chat → Graph | AI 响应中出现 Wiki 路径链接 | 图谱高亮对应节点 |
| Graph → Chat | 右键节点 → "问这个" | Chat 打开并预填上下文 |
| Graph → Ask | 右键节点 → "检索相关" | Ask 搜索框预填节点标题 |

---

## 2. 技术背景

### 2.1 现有结构

- `web/src/modules/graph/` — 图谱模块，使用 `cytoscape.js` 渲染节点/边
- `web/src/modules/chat/` — Chat 模块，AI 对话
- `web/src/modules/ask/` — Ask 搜索模块
- `web/src/App.tsx` — 顶级路由/模式切换，持有 `activeModule` 状态
- `web/src/contexts/` — 现有 `RuntimeContext`, `VaultContext`, `ModeContext`

### 2.2 通信机制选择

模块间通信有三种可选方案：

| 方案 | 优点 | 缺点 |
|------|------|------|
| 新增 `GraphChatContext` | 解耦，双向订阅 | 新增 Context provider，需要在 App.tsx 注入 |
| 事件总线（EventEmitter） | 无 React 层级限制 | 引入额外依赖或自实现 |
| URL hash/params | 刷新后可恢复 | 路由耦合，不适合 Tauri |

**选择**：新增轻量 `GraphBridgeContext`（两个 `useState` + 一个 `useRef`），无额外依赖。

---

## 3. 实施方案

### Phase A：Chat → Graph（高亮联动）

#### 3A.1 GraphBridgeContext

新建 `web/src/contexts/GraphBridgeContext.tsx`：

```typescript
interface GraphBridgeState {
  highlightedPaths: string[];   // Chat 中最近一条 AI 响应引用的 wiki 路径
  setHighlightedPaths: (paths: string[]) => void;
  focusedNode: string | null;   // Graph 节点被右键选中后传给 Chat 的路径
  setFocusedNode: (path: string | null) => void;
}
```

#### 3A.2 Chat 响应解析 Wiki 路径

在 `MessageBubble.tsx` 中，AI 响应渲染时用正则提取 `[[path]]` 或 Markdown 链接：

```typescript
const WIKI_LINK_RE = /\[\[([^\]]+)\]\]|\[([^\]]+)\]\(wiki:\/\/([^)]+)\)/g;

function extractWikiPaths(content: string): string[] {
  const paths: string[] = [];
  for (const m of content.matchAll(WIKI_LINK_RE)) {
    paths.push(m[1] || m[3]);
  }
  return [...new Set(paths)];
}
```

AI 响应完成后（`done` 事件），调用 `setHighlightedPaths(extractWikiPaths(content))`。

#### 3A.3 Graph 消费高亮状态

在 `GraphView.tsx`（或 `GraphCanvas.tsx`）中：

```typescript
const { highlightedPaths } = useGraphBridge();

useEffect(() => {
  if (!cy) return;
  cy.nodes().removeClass("highlighted");
  highlightedPaths.forEach(p => {
    cy.nodes(`[path = "${p}"]`).addClass("highlighted");
  });
}, [highlightedPaths, cy]);
```

CSS：
```css
.cy-node.highlighted { border-width: 3px; border-color: var(--accent); }
```

---

### Phase B：Graph → Chat（右键"问这个"）

#### 3B.1 图谱节点右键菜单

使用 `cytoscape-cxtmenu` 插件（已是 cytoscape 生态，轻量）或自实现右键菜单：

```typescript
cy.on("cxttap", "node", (evt) => {
  const node = evt.target;
  const nodePath = node.data("path");
  const nodeTitle = node.data("label");
  showContextMenu({ x: evt.renderedPosition.x, y: evt.renderedPosition.y, nodePath, nodeTitle });
});
```

右键菜单选项：
1. **问这个** → `setFocusedNode(nodePath)` + 切换到 Chat 模块
2. **检索相关** → 切换到 Ask 模块并预填 `nodeTitle`
3. **查看详情** → 打开 Wiki 页面（已有功能）

#### 3B.2 Chat 模块接收节点上下文

Chat 输入框检测 `focusedNode` 是否变化：

```typescript
const { focusedNode, setFocusedNode } = useGraphBridge();

useEffect(() => {
  if (!focusedNode) return;
  setInputDraft(`请基于 [[${focusedNode}]] 这个主题`);
  setFocusedNode(null);  // 消费后清零
  inputRef.current?.focus();
}, [focusedNode]);
```

---

### Phase C：Graph → Ask（右键"检索相关"）

```typescript
// 在 App.tsx 维护 askPrefill 状态
const [askPrefill, setAskPrefill] = useState<string>("");

// Graph 右键"检索相关"
onSearchRelated(nodeTitle: string) {
  setAskPrefill(nodeTitle);
  setActiveModule("ask");
}
```

Ask 模块初始化时消费 `askPrefill`，触发搜索后清零。

---

## 4. 文件变动清单

| 文件 | 操作 | 说明 |
|------|------|------|
| `web/src/contexts/GraphBridgeContext.tsx` | 新建 | 双向桥接状态 |
| `web/src/main.tsx` | 修改 | 注入 `<GraphBridgeProvider>` |
| `web/src/modules/chat/MessageBubble.tsx` | 修改 | AI 响应完成时提取 wiki 路径，更新高亮 |
| `web/src/modules/graph/GraphView.tsx` | 修改 | 消费高亮状态，节点右键菜单 |
| `web/src/modules/graph/graph.css` | 修改 | `.highlighted` 节点样式 |
| `web/src/modules/ask/AskPanel.tsx` | 修改 | 接收 `askPrefill` 预填 |
| `web/src/App.tsx` | 修改（最小） | 传递 `askPrefill`/`setAskPrefill`，模块切换 |

**不需要修改任何 Rust 后端文件。**

---

## 5. 验收标准

- [ ] Chat 中 AI 响应完成后，Graph 中对应节点有明显高亮（颜色/边框变化）
- [ ] 切换到其他 AI 响应时，高亮更新（旧节点恢复正常）
- [ ] Graph 节点右键显示"问这个"/"检索相关"菜单
- [ ] 点击"问这个"：Chat 模块激活 + 输入框预填节点路径上下文
- [ ] 点击"检索相关"：Ask 模块激活 + 搜索框预填节点标题并执行搜索
- [ ] `npm run typecheck` 零错误

---

## 6. 风险与注意事项

1. **cytoscape 节点数据结构**：高亮依赖 node `data("path")` 字段，需确认图谱节点初始化时是否包含此字段（若 key 是 `id` 而非 `path`，正则匹配需要调整）
2. **Graph 模块懒加载**：如果 Graph 使用 `React.lazy()`，`useGraphBridge()` 在图谱未渲染时调用需确保 Context 已就绪
3. **AI 响应流式输出期间不触发高亮**：只在 `done` 事件后提取，避免频繁更新图谱

---

## 7. 工作量估算

| Phase | 估算 | 关键风险 |
|-------|------|---------|
| A（Chat→Graph 高亮） | 0.5 天 | cytoscape API 熟悉度 |
| B（Graph→Chat 右键） | 0.5 天 | 右键菜单实现方式 |
| C（Graph→Ask 预填） | 0.25 天 | 极低风险 |
| 测试 + 调整 | 0.25 天 | — |
| **总计** | **~1.5 天** | — |
