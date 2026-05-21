# H25 · Deep Research 全面升级计划

> 创建：2026-05-21 | 执行人：Claude Code | 状态：进行中

---

## 目标总览

| 编号 | 模块 | 核心改动 | 优先级 |
|------|------|----------|--------|
| P1 | 报告格式规范化 | synthesis prompt 带编号引用 + References 格式化 | ★★★ |
| P2 | 多搜索提供商并行 | SearchConfig 多选 + 并行执行 + 前端 checkboxes | ★★★ |
| P3 | 开源项目调研 | gpt-researcher / STORM 关键能力提取 | ★★ |
| P4 | 独立 HTML 导出 | 单文件 HTML + Tauri pickSaveFile + 前端按钮 | ★★ |
| UI | ResearchPanel 美化 | 卡片、进度条、按钮、状态徽章全面升级 | ★★ |

---

## P1 · 报告格式规范化

### 问题
- 正文无引用编号，结论无法溯源
- References 只是裸 URL 列表
- 学术论文和普通网页参考文献格式相同

### 实施

**文件**：`src-tauri/src/state/research_service.rs`

1. **源编号注入**：在 `round_snippets` 构建时使用全局编号（跨轮次不重置）
2. **Synthesis Prompt 更新**：
   - 明确要求正文 `[N]` 标注
   - 附带完整编号源列表（title + url + snippet 前 200 字）
   - 强制输出规范 References 区块
3. **References 后处理**：
   - 检测 URL 是否含 `doi.org` / `arxiv.org` / `pubmed` → 学术格式
   - 其他 → 网页格式 `[N] Title. Site. URL. (YYYY-MM-DD)`
   - 学术格式 `[N] Author (Year). Title. *Venue*. URL`

### 新 synth_prompt 结构
```
You are a professional research analyst. Write a comprehensive report.

Topic: {topic}

Research Sources (cite inline as [N]):
[1] Title (domain.com): snippet...
[2] Title (arxiv.org): snippet...
...

Key Research Findings:
1. ...

Requirements:
- Every factual claim MUST have [N] inline citation
- Final section "## References" with properly formatted entries
- Academic papers (arxiv/doi/pubmed): Author (Year). Title. Venue. URL
- Web articles: [N] Title. Site Name. URL. Accessed {date}
- Write in same language as topic title
```

---

## P2 · 多搜索提供商并行

### 问题
- SearchConfig.search_provider 单选字符串
- 只能用一个提供商，任一失败则整体失败

### 实施

**后端文件**：
- `src-tauri/src/models.rs`：新增 `search_providers: Vec<String>`（serde default 空向量；读时若空则从旧字段迁移）
- `src-tauri/src/state/search_service.rs`：新增 `do_search_multi` 并行查所有提供商，按 URL 去重合并
- `src-tauri/src/state/research_service.rs`：将 `do_search` 替换为 `do_search_multi`
- `validate_search_config` 更新：任一提供商配置有效即通过

**前端文件**：
- `web/src/modules/lint/SearchConfigPanel.tsx`：radio → 两个独立 checkbox（Tavily / SearXNG）
- `web/src/types.ts`：`search_providers: string[]` 字段
- `web/src/tauri-client/search.ts`：setSearchConfig 透传新字段

### 数据迁移策略
```rust
// 读取时：若 search_providers 为空，从旧 search_provider 字段迁移
pub fn effective_providers(&self) -> Vec<String> {
    if !self.search_providers.is_empty() {
        self.search_providers.clone()
    } else if self.search_provider != "none" {
        vec![self.search_provider.clone()]
    } else {
        vec![]
    }
}
```

---

## P3 · 开源项目调研

**调研目标**：
- [assafelovic/gpt-researcher](https://github.com/assafelovic/gpt-researcher)
- [stanford-oval/storm](https://github.com/stanford-oval/storm)

**输出**：调研笔记追加至本文件末尾，提取可落地能力点

---

## P4 · 独立 HTML 单文件导出

### 实施

**后端文件**：
- `src-tauri/src/commands.rs`：新增 `export_research_html` 命令
- `src-tauri/src/state/research_service.rs`：新增 `export_research_html_file` 函数
  - 读取 .md → `pulldown-cmark` 转 HTML
  - 内嵌 CSS（可读性优化：max-width 800px, 衬线字体, 代码高亮）
  - 生成浮动目录（从 h2/h3 标题提取锚点）
  - References 中 URL 转可点击 `<a>` 标签
  - 调用 `pickSaveFile` 弹保存对话框
  - 写入用户选择路径

**前端文件**：
- `web/src/modules/research/ResearchPanel.tsx`：Done 状态卡片加"导出 HTML"按钮
- `web/src/tauri-client/search.ts`：新增 `exportResearchHtml` 函数

---

## UI · ResearchPanel 全面美化

**目标**：任务卡片更有层次感，日志区 terminal 风格，状态徽章突出，操作按钮统一风格

**文件**：
- `web/src/modules/research/ResearchPanel.tsx`
- `web/src/modules/research/research.css`

**改造点**：
1. 任务卡片：左侧状态竖线 accent 色，hover 略微阴影，Failed/Done 背景色微差异
2. 日志区：深色 terminal 风格背景（monospace字体），进度行有图标前缀
3. 状态徽章：`完成` 绿色 pill，`失败` 红色 pill，`运行中` 紫色 pulse 动画
4. 操作按钮：与 export-card__btn 风格统一（"查看 Wiki"/"导出 Word"/"删除"各有语义色）
5. 研究选项 pills（深度/广度）更圆润，active 态更明显

---

## 执行顺序与并行策略

```
[并行 Group-1]
  Agent-A: P1 (research_service.rs synthesis)
  Agent-B: P2 (models + search_service + SearchConfigPanel)
  Agent-C: P3 (web research, read-only)
  Main:    UI 美化 (ResearchPanel + research.css)

[串行 Group-2，Group-1 完成后]
  Main: P4 HTML 导出 (research_service + commands + ResearchPanel button)

[收口]
  Build 验证 → 测试 → commit → 简报
```

---

## 进度追踪

- [ ] P1 报告格式 — synthesis prompt + citation template
- [ ] P2 多搜索提供商 — 后端并行 + 前端 checkbox
- [ ] P3 调研笔记
- [ ] P4 HTML 导出
- [ ] UI 美化
- [ ] Build 验证通过
- [ ] Git commit 完成

---

## P3 调研笔记

### GPT-Researcher 核心机制
1. **树形递归探索**：breadth × depth × 并发，异步并行，自动跳过失败查询
2. **多源融合**：20+ 搜索源聚合去重，全链路引用追踪（planner→execution→publisher）
3. **多 Agent 分工**：Chief Editor / Researcher / Reviewer / Writer / Publisher 协作

### STORM（Stanford）核心创新
1. **Outline-First 两阶段**：先生成大纲（研究+视角），再写作（内容+引用），质量提升 25%
2. **视角驱动**：从同类 Wikipedia 文章发现多研究视角，模拟"编者⟷专家"对话追问
3. **引用验证**：Mistral 7B 检查 textual entailment，确保每条引用有文本支撑

### 近期可落地建议（优先级）
- **P0**：Outline-First 架构（先大纲再扩展，已在 P1 合成 prompt 中体现章节结构要求）
- **P0**：引用精准度（P1 已实现 [N] 标注，可进一步加后处理验证层）
- **P1**：多视角初始查询（在分解阶段增加 perspective discovery prompt）
- **P2**：Reviewer 代理角色（发布前二次验证引用和逻辑）

### 与现有 dzhng/deep-research 区别
- GPT-Researcher 强调横向扩展（速度+多源）
- STORM 强调纵向深化（结构+质量）
- 建议：融合两者 —— STORM 大纲优先提升结构，GPT-Researcher 并行 tree-search 加速
