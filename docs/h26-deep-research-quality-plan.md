# H26 · Deep Research 质量升级（基于真实 GitHub 调研）

> 创建：2026-05-21 | 起草：Claude Code（Opus 4.7）  
> 执行模型：Sonnet 4.6（具体编码）+ Codex（按需后端深度修改）  
> 上游：H25 已落地（[N] 引用、多搜索、HTML 导出、UI 美化，commit `d34465b`）  
> 本计划：基于 5 个真实 GitHub 项目的源码与 README 抽取出的差距，4 项可落地改进 + 1 项延后规划

---

## 0. 调研来源（真实读过的项目）

本计划改进点不是凭训练数据猜测，而是 2026-05-21 实地 fetch 的真实结论：

| 项目 | URL | 我们学到了什么 |
|------|-----|----------------|
| **gpt-researcher** | https://github.com/assafelovic/gpt-researcher | Planner→Executor→Publisher 三层；20+ 数据源并行去重；学术 vs 网页参考文献分格式 |
| **stanford-oval/storm** | https://github.com/stanford-oval/storm | Outline-First 两阶段（先大纲再写作，质量 +25%）；视角驱动多角度提问 |
| **bytedance/deer-flow** | https://github.com/bytedance/deer-flow | Sub-agent 上下文隔离；progressive skill loading；中间结果落盘节省 token |
| **LearningCircuit/local-deep-research** | https://github.com/LearningCircuit/local-deep-research | LangGraph 自适应路由策略（**SimpleQA 95%**）；arXiv/PubMed/Semantic Scholar 等学术源 |
| **langchain-ai/open_deep_research** | https://github.com/langchain-ai/open_deep_research | Supervisor + 并行 sub-agent；Deep Research Bench #6 |

辅助索引：
- https://github.com/DavidZWZ/Awesome-Deep-Research（Awesome 清单，未来追新项目入口）

---

## 1. 目标总览

| 编号 | 模块 | 核心改动 | 优先级 | 难度 |
|------|------|----------|--------|------|
| **H26-B** | 学术数据库接入 | arXiv + Semantic Scholar 两个免费 REST API | ★★★ | 低 |
| **H26-A** | Outline-First 架构 | 先大纲→章节搜索→章节写作→拼装 | ★★★ | 中 |
| **H26-C** | 来源质量评分 | 域名加权、合成时优先高分源 | ★★ | 低 |
| **H26-E** | 分章节进度推送 | 事件粒度从轮次细化到章节级 | ★ | 低 |
| H27（延后） | 自适应追踪搜索 | LLM 在每轮判断追加新查询 | — | 高 |

**实施顺序**：先做 B（最快见效）→ 顺手做 C（B 中可一并完成）→ 再做 A（结构改造）→ 收口做 E（用户体验）。

H27 暂不开工，先记录到 §6 防遗忘。

---

## 2. H26-B · 学术数据库接入

### 2.1 现状

`SearchConfig.search_providers` 当前可选 `tavily` / `searxng`，都是通用网页搜索。研究"量子纠缠"、"BERT 对比 GPT"、"肿瘤免疫疗法"这类话题时，命中的常是科普博客、知乎答案，而非真正的论文。

### 2.2 目标

研究学术/技术话题时，能自动拉到 arXiv 论文摘要 + Semantic Scholar 同行评审来源，与 Tavily/SearXNG 结果合并去重统一处理。

### 2.3 API 调研结论

#### arXiv API（完全免费、无 key）

- 端点：`http://export.arxiv.org/api/query`
- 参数：`search_query=all:<query>&start=0&max_results=10&sortBy=relevance`
- 返回：Atom XML（含 title / summary / authors / published / id 即 PDF/HTML URL）
- 限速：建议 3 秒一次（官方建议）

#### Semantic Scholar API（免费、可选 key 提升限速）

- 端点：`https://api.semanticscholar.org/graph/v1/paper/search`
- 参数：`?query=<query>&limit=10&fields=title,abstract,authors,year,url,venue,citationCount`
- 返回：JSON
- 限速：无 key 时 100 req / 5min；有 key 1000 req / s
- 注：无 key 时也能直接用，对个人 Wiki 场景够用

#### 不接入的（理由）

- PubMed：需医学领域才有价值，先不上
- NASA ADS：天文专用，过窄
- Google Scholar：无官方 API，第三方爬虫不稳定

### 2.4 实施清单

**后端：新增 `src-tauri/src/state/search_service.rs` 函数**

```rust
// 新增 provider 标识：'arxiv', 'semantic_scholar'
pub(super) async fn search_arxiv(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
) -> Result<Vec<WebSearchResult>, String>

pub(super) async fn search_semantic_scholar(
    client: &reqwest::Client,
    query: &str,
    limit: usize,
    api_key: Option<&str>,
) -> Result<Vec<WebSearchResult>, String>
```

**`do_search_multi` 改造**：根据 `effective_providers()` 里的字符串扇出到对应的 provider 函数；`WebSearchResult` 中**新增字段 `source_type: String`**（`"academic"` / `"web"`）。

**`SearchConfig` 扩展**：

```rust
pub struct SearchConfig {
    pub search_provider: String,         // 旧字段保留兼容
    pub search_providers: Vec<String>,   // 现可含 "arxiv" / "semantic_scholar"
    pub semantic_scholar_api_key: Option<String>,  // 新增，可空
    // ... 其它字段
}
```

**前端：`web/src/modules/lint/SearchConfigPanel.tsx`**

将 checkbox 从 2 个扩展为 4 个：
- `[x] Tavily（通用网页）`
- `[x] SearXNG（通用网页，本地）`
- `[ ] arXiv（学术论文，免费无 key）`
- `[ ] Semantic Scholar（同行评审，免费无 key，可选 API key）`

Semantic Scholar 勾选时显示"API Key（可选，留空走匿名限速）"输入框。

**`web/src/types.ts`**：`semantic_scholar_api_key?: string | null`。

### 2.5 验收

1. 用 "Retrieval Augmented Generation" 做研究，能看到 arxiv.org 域名的来源出现在 References
2. References 区按现有学术格式（H25 P1 已实现的 `Author (Year). Title. *Venue*. URL`）输出
3. Settings → Search Config 能勾选/取消 arXiv 和 Semantic Scholar，配置可保存重启不丢
4. arXiv API 临时不可用时，研究流程不中断（其它 provider 仍工作）
5. cargo test 全绿（新增 ≥2 个单元测试：XML 解析、JSON 解析）
6. typecheck 零错误

---

## 3. H26-A · Outline-First 报告架构

### 3.1 现状

```
当前：[topic] → 一次性生成 N 个子查询 → N 轮搜索 → 一次性合成完整报告
```

问题：报告章节是 LLM 在最后一步即兴划分的，不同话题质量差异大；遇到主题边界模糊时容易跑题或重复。

### 3.2 目标（参考 STORM）

```
新：[topic]
    → Phase 1: 生成研究大纲（3-5 章 + 每章核心问题）
    → 用户审批大纲（沿用现有 register_query_approval 机制扩展）
    → Phase 2: 针对每章并行搜索 + 章节内合成
    → Phase 3: 拼装 + Introduction + Conclusion + References
```

### 3.3 数据结构

```rust
struct ResearchOutline {
    title: String,
    sections: Vec<OutlineSection>,
}

struct OutlineSection {
    heading: String,                    // 例如 "## 2. 核心机制"
    key_questions: Vec<String>,         // 该章节要解决的 3-5 个具体问题
    search_queries: Vec<String>,        // 由 key_questions 派生的搜索词
}

struct SectionDraft {
    heading: String,
    body: String,                       // 已带 [N] 引用
    used_sources: Vec<usize>,           // 引用的 source_index 列表
}
```

### 3.4 流程改造（`src-tauri/src/state/research_service.rs`）

**保留**：`start_research_task` 入口、任务状态管理、`source_index` 全局编号、`register_query_approval` 审批机制。

**改造主循环**：

```text
Phase A: outline 生成（新增）
  prompt: "为话题 X 生成研究大纲，要求 N 个章节，每章 3 个关键问题，每问题对应 1-2 个搜索词。返回 JSON。"
  emit_progress("planning_outline", ...)
  注册 outline 审批：复用 register_query_approval，扩展事件 research_outline_ready

Phase B: 章节并行搜索（替代当前 depth 循环）
  for each section in outline.sections:
    spawn:
      多个并行查询（含原来的 do_search_multi）
      累积 round_results 到 section.results
      emit_progress("researching_section", "第 N 章：xxx")
  
Phase C: 章节内合成（新增）
  for each section in outline.sections:
    prompt: "用以下来源 [编号列表] 写 section.heading，要求 [N] 行内引用，目标 800-1500 字"
    sections.push(SectionDraft { ... })

Phase D: 报告拼装（替代当前 synth_prompt）
  prompt: "为 topic 写 Introduction（300字）和 Conclusion（300字）"
  组装：Title + Introduction + 各 SectionDraft.body + Conclusion + References
  References 由所有 source_index 排序输出（沿用 H25 学术/网页双格式）
```

### 3.5 前端改动

`ResearchPanel.tsx` 大纲审批步骤：

- 在现有"查询审批"步骤之后插入"大纲审批"
- 前端事件监听 `research_outline_ready`，弹出 dialog 展示章节标题 + 关键问题，允许用户编辑后批准
- 类似现有 `approve_research_queries`，新增 `approve_research_outline` Tauri 命令

### 3.6 兼容性

- 报告 Markdown 输出格式与 H25 一致（Frontmatter + 正文 + References）
- 数据库 `research_tasks` 表无需迁移；如需保存大纲，扩展 `meta_json` 字段（已是 JSON）

### 3.7 验收

1. 同一话题（如"Tauri vs Electron"）走 Outline-First 后，章节划分明显比 H25 更结构化
2. 大纲审批 dialog 可以编辑章节标题、删除章节、修改 key_questions
3. References 编号跨章节连续，不重复
4. 旧任务（H25 创建的）打开仍正常显示，不会因数据结构变化崩溃
5. cargo test 全绿（新增 outline 解析测试、section 合成测试）
6. typecheck 零错误

---

## 4. H26-C · 来源质量评分

### 4.1 设计

`WebSearchResult` 新增 `quality_score: f32`（0.0–1.0），由 URL 域名决定：

```rust
fn score_by_domain(url: &str) -> f32 {
    let host = url_hostname(url);  // 已存在
    match () {
        _ if host.contains("arxiv.org") => 0.95,
        _ if host.contains("semanticscholar.org") => 0.95,
        _ if host.contains("doi.org") || host.contains("pubmed") => 0.9,
        _ if host.ends_with(".edu") || host.ends_with(".gov") => 0.85,
        _ if host.contains("nature.com") || host.contains("sciencedirect")
            || host.contains("ieee.org") || host.contains("acm.org") => 0.85,
        _ if host.contains("wikipedia.org") => 0.7,
        _ if host.contains("github.com") || host.contains("stackoverflow") => 0.65,
        _ if host.contains("medium.com") || host.contains("dev.to")
            || host.contains("zhihu") || host.contains("csdn") => 0.4,
        _ => 0.5,
    }
}
```

### 4.2 应用点

- `do_search_multi` 合并后按 `quality_score` 降序排序，截断 top-N（N 由 breadth 决定）
- 合成 prompt 时高分源放前面，并加注释 `[1] (high quality, arxiv.org): ...`
- 报告 References 输出时不显示分数（仅内部用）

### 4.3 验收

1. 同一查询，arxiv 来源排在 medium 来源前
2. cargo test：新增 `score_by_domain` 单元测试覆盖 10+ 域名

---

## 5. H26-E · 分章节进度推送

### 5.1 当前事件

- `research_progress`：stage + message（粗粒度）
- `research_queries_ready`：子查询就绪
- `research_error`：失败

### 5.2 新增/扩展

- `research_outline_ready` (新)：大纲就绪，等待审批
- `research_progress` 的 stage 扩展：
  - `planning_outline` — 生成大纲中
  - `awaiting_outline_approval` — 等待大纲确认
  - `researching_section` — payload 加 `section_index`, `section_title`, `total_sections`
  - `writing_section` — payload 加 `section_index`, `section_title`
  - `assembling` — 拼装最终报告

### 5.3 前端（`ResearchPanel.tsx`）

任务卡片日志区显示形如：
```
$ planning_outline   规划研究大纲...
$ outline_ready      ✓ 5 个章节就绪
$ researching_section 第 2/5 章：核心机制
  ✓ tavily（3 条） ✓ arxiv（4 条）
$ writing_section    第 2/5 章写作中...
```

---

## 6. H27（延后）· 自适应追踪搜索

> ⚠️ 本计划**不实施**此项，仅记录避免遗忘。实施时单独开 H27 计划。

### 设计要点

参考 local-deep-research 的 LangGraph 策略：

```
每轮搜索结束后：
  LLM 评估当前已有信息 + 新发现的 learnings
  → 决策：CONTINUE（追加查询）/ STOP（信息已足够）
  → 若 CONTINUE：LLM 生成 1-3 个针对性新查询并加入下一轮
循环上限 5 轮
```

### 实施难度

- 主循环要重构为状态机
- 需要新的 prompt：评估信息充分度
- 测试覆盖度要求高（容易死循环）

### 触发时机

H26 落地稳定后；优先级排在 H10 Phase A、ONNX 模型在线下载之后。

---

## 7. 执行总览

```
Day 1   ┌─ H26-B 后端（Codex 或 Sonnet）：arxiv + semantic_scholar fetch + 解析
        │  └─ 并行：H26-C（域名打分，纯函数）
        └─ H26-B 前端（Sonnet）：4 个 checkbox + key 输入

[阶段提交：H26-B + H26-C 完成]

Day 2-3 ┌─ H26-A 后端（Sonnet 主导，Codex 协助）：outline 生成 + 章节并行 + 章节合成
        └─ H26-A 前端（Sonnet）：outline 审批 dialog + approve_research_outline 命令

[阶段提交：H26-A 完成]

Day 4   H26-E 进度事件细化 + UI 适配（Sonnet）
        最终验证 → 合并提交 → 简报

合计估时：3-4 天（并行后约 2.5 天）
```

---

## 8. 与三方协作模型的同步

按 `agents.md §16.4` 协议：

- **本计划文件**：`docs/h26-deep-research-quality-plan.md`（本文）
- **dev-status.md**：本轮起加入"活跃 TODO H26"，标注上游 commit `d34465b`
- **实施过程记录.md**：每完成一个 H26-X 追加一条
- **Codex / Gemini**：从 `dev-status.md` 看到 H26 项目后，按本计划文件分工
- **基线验证**：Windows PowerShell 下 `cd src-tauri; cargo test` + `cd web; npm run typecheck`

---

## 9. 进度追踪

- [ ] H26-B 学术 API 接入（arXiv + Semantic Scholar）
- [ ] H26-C 来源质量评分
- [ ] H26-B/C 阶段 commit
- [ ] H26-A Outline-First 后端
- [ ] H26-A Outline-First 前端
- [ ] H26-A 阶段 commit
- [ ] H26-E 进度事件细化
- [ ] 最终全量基线验证
- [ ] 收口 commit + 简报

---

## 10. 风险与回滚

| 风险 | 概率 | 应对 |
|------|------|------|
| arXiv API 阻塞或封锁 | 低 | 客户端 8s 超时；失败不影响其它 provider |
| Semantic Scholar 限速 | 中 | 无 key 时单次研究最多查 5 次；提供 key 输入 |
| Outline 生成质量不稳 | 中 | 失败时降级到 H25 行为（直接 sub-query 模式） |
| 数据库结构变更 | 低 | 仅扩展 meta_json，不动 schema |
| 报告 Markdown 格式变化破坏旧任务 | 低 | 保持 Frontmatter + References 区结构与 H25 一致 |

回滚策略：每阶段独立 commit，任一阶段不通过可单独 revert。
