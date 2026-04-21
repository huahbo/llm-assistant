---
type: clip
title: "Karpathy 提出了 LLM Wiki，我用 Rust 把它造出来了，还打通了 MemPalace 知识图谱"
url: "https://mp.weixin.qq.com/s/LxgNWJ_FUFOxA3DCdTgquw"
clipped: 2026-04-21
origin: clipper
tags: [clip]
---

# Karpathy 提出了 LLM Wiki，我用 Rust 把它造出来了，还打通了 MemPalace 知识图谱

Source: https://mp.weixin.qq.com/s/LxgNWJ_FUFOxA3DCdTgquw

Karpathy 提出了 LLM Wiki，我用 Rust 把它造出来了，还打通了 MemPalace 知识图谱

原创

小张

小张

老码小张

2026年4月17日 00:30

广东

在小说阅读器读本章

去阅读

在小说阅读器中沉浸阅读

上一篇文章，我做了一个 Rust 版本的记忆系统，我就在思考怎么和Karpathy老师的这个 llm wiki 结合起来，当然，我不只是想做概念验证，我思考的是是一个覆盖事件驱动、可审计、三路 RRF 融合检索，并已接入 DeepSeek 完成端到端测试，嗯，我很喜欢这个，因为他很便宜，也很适合做这个事情。顺着这个问题，我先说一个让我抓狂的问题我不知道你可有遇到过这种情况：跟 ChatGPT 讨论了半小时项目背景，关掉对话窗口——全没了。下次再聊，重新解释一遍。再下次，再解释一遍。或者用 RAG 搭了个知识库，把文档上传进去，然后发现模型每次回答都像「第一次看到这些文件」，没有任何积累，没有任何演进。这就是当前主流 LLM + 知识管理方案的根本缺陷：知识不会累积。所以，Karpathy 点了一把火前不久 Andrej Karpathy 在 GitHub Gist 上分享了一个想法，叫 LLM Wiki。我很早其实就注意到了，就想能够用到 Agent 上面来。其实，他的核心理念一句话：与其每次 query 时让模型重新从文档堆里推导答案，不如让模型持续维护一个「活的 wiki」——每次读入新资料，就更新相关页面、标注矛盾、强化知识连接。知识只要编译一次，就永久留存并不断复利。他把这套流程描述成三层：• Raw sources：你的原始资料，只读不改• Wiki：模型持续生成和维护的 Markdown 页面集合• Schema：约定规范，让模型成为称职的「知识馆员」而非随意聊天的工具这个想法极其迷人，但 Karpathy 的 Gist 只是一份概念文档——没有代码，没有工程落地，更没有生产级的可靠性保证。所以我决定用 Rust 把它造出来，并且在工程层面超越它。所以，我做了什么这整个项目叫 llm-wiki，是一个纯 Rust workspace，分为五个 crate：wiki-core          # 领域模型（纯函数，零 IO）wiki-kernel        # 编排引擎 + wiki 投影层wiki-storage       # SQLite 持久化 + outboxwiki-mempalace-bridge  # 外部知识图谱接口wiki-cli           # 命令行工具架构如下：Raw Sources    │    ▼IngestPipeline ──▶ WikiKernelEngine ──▶ SQLite State                         │                         ├──▶ Outbox Events ──▶ External Consumers                         │                         └──▶ MarkdownWikiWriter                                    │                                    ├── index.md                                    ├── log.md                                    ├── pages/                                    ├── concepts/                                    └── reports/只是完全实现Karpathy老师的构想可能还不够，不过先走一步知识有生命周期，不是一写了之Karpathy 的想法里，知识就是 Markdown 文本。但是我在实现的时候，我想，我们的知识是有结构的断言（Claim）：pub struct Claim {    pub tier: MemoryTier,       // Working → Episodic → Semantic → Procedural    pub confidence: f64,        // 综合置信度    pub quality_score: f64,     // 质量分    pub supersedes: Option<ClaimId>,  // 取代关系    pub stale: bool,            // 是否过时    pub access_count: u32,      // 访问次数（用于保留强度加权）}每一条知识都有层级（类比人类记忆：工作记忆 → 情节记忆 → 语义记忆 → 程序性记忆），有置信度，有半衰期。旧的结论不删除，而是被「取代」，因为旧 claim 标记 stale，新 claim 记录 supersedes 引用，全程可追溯。检索用的是三路融合，不是单一向量搜索query 时走三路并行召回：• BM25（关键词）• Vector（语义向量）• Graph（知识图谱游走）三路结果用 RRF（Reciprocal Rank Fusion） 合并，再叠加保留强度加权（越常被访问、越新的 claim 权重越高）。score_final = rrf_score × retention_strength(claim, now)这让检索结果既精准又记得住什么重要。然后我做了，事件驱动，而不是黑盒调用系统里所有写操作都会产生事件，落入 wiki_outbox 表：SourceIngested / ClaimUpserted / ClaimSupersededPageWritten / QueryServed / LintRunFinished / SessionCrystallized消费端可以按 offset 增量拉取，可以幂等 ack，可以桥接到外部知识图谱（比如我同步维护的 rust-mempalace）。对，这个地方是我想到的，把这个系统和记忆系统连接起来。这是 Karpathy老师提供的的方案中，没有考虑到的一个工程层，也就是你知道知识库发生了什么、什么时间发生的、谁消费了。另外，我还设计了自动 Lint，让知识库会自检内置 lint 操作，会主动发现：• page.broken_wikilink：页面里引用了不存在的 [[页面]]• page.orphan：没有任何页面指向它（孤岛页面）• claim.stale：过时 claim 没有在页面里被妥善处理• xref.missing：claim 关键词在现有页面里找不到对应引用结果生成结构化报告文件（wiki/reports/lint-*.md），供人工或模型回头处理。随后，我做了一个完整的审计轨迹每一次 ingest、写 claim、supersede、query、crystallize，都写入 AuditRecord，actor、时间、摘要一条都不少。知识库的每一步演化都有据可查。那么，如何快速上手这个项目# ingest 一份资料，并同步生成 wiki 投影cargo run -p wiki-cli -- \  --db wiki.db --wiki-dir wiki --sync-wiki \  ingest "file:///notes/redis.md" "项目使用 Redis 做缓存" --scope private:me# 写入一条知识断言cargo run -p wiki-cli -- --db wiki.db \  file-claim "Redis 默认 TTL 为 3600 秒" --tier semantic# query，结果可选落盘为 wiki 页面cargo run -p wiki-cli -- --db wiki.db --wiki-dir wiki --sync-wiki \  query "Redis 配置策略" --write-page --page-title "analysis-redis"# 健康检查cargo run -p wiki-cli -- --db wiki.db --wiki-dir wiki --sync-wiki lint生成的 wiki/ 目录可以直接用 Obsidian 打开——graph view、[[wikilink]] 跳转、Dataview 插件都能无缝接入。我默认接入 DeepSeek（其实，任意 OpenAI-compatible 模型都 ok）填写 llm-config.toml：[llm]base_url = "https://api.deepseek.com/v1"api_key  = "你的 key"model    = "deepseek-chat"为了健壮性，我做了冒烟测试：cargo run -p wiki-cli -- llm-smoke --config llm-config.toml --prompt "Say 'ok' only."# 输出：ok看看我的测试覆盖# 单元测试（14 个用例，全绿）cargo test# 端到端回归（8 步完整链路）./scripts/e2e.she2e 脚本覆盖：ingest → file-claim → supersede → query → lint → outbox export → ack → mempalace 消费（断言 consumed > 0）→ DeepSeek 冒烟。我对于技术选型的说明选择理由Rust零成本抽象、类型系统在领域建模上极其顺手、无 GC 适合长期驻留的知识引擎SQLite单文件、可随 wiki 一起 git 管理、WAL 模式够用Outbox 模式状态先落地、事件后追记，消费端幂等，at-least-once 语义Workspace 分 cratewiki-core 纯函数零 IO，测试最简单；wiki-kernel 编排；wiki-storage 可替换我想说的是，这仅仅是一个起点，不是终点目前还有很多有趣的事情可以做：• 接入真实向量库（当前 BM25/vector 是 stub，等待注入真实实现）• LLM 驱动的自动 ingest（模型读完资料后自动提取 claim 并更新 wiki 页面）• 多 agent 协作（Scope::Shared 已经预留了 team_id）• 与 rust-mempalace 深度联动（知识图谱 + RRF 融合）写在最后Karpathy 给了一个极好的概念，我们给了它一个工程内核。一个真正"不会遗忘"的知识系统，不应该只停留在 Gist 里。代码在仓库里，测试全绿，欢迎来一起学习。项目地址：https://github.com/coder-brzhang/llm-wiki注意，本项目仅在小张的400 多个人的小群（公众号菜单-联系我-加群）中分享。用 Rust 写的，用 DeepSeek 测的，用 Obsidian 看的。

预览时标签不可点

关闭更多名称已清空微信扫一扫赞赏作者喜欢作者其它金额赞赏后展示我的头像作品暂无作品喜欢作者其它金额¥最低赞赏 ¥0确定返回其它金额更多赞赏金额¥最低赞赏 ¥01234567890.  关闭更多搜索「」网络结果

关闭调整当前正文文字大小更多100%

​留言暂无留言1条留言已无更多数据发消息   写留言:

微信扫一扫关注该公众号

继续滑动看下一个

轻触阅读原文

老码小张

向上滑动看下一个

当前内容可能存在未经审核的第三方商业营销信息，请确认是否继续访问。继续访问取消微信公众平台广告规范指引

知道了

微信扫一扫使用小程序

取消

允许

取消

允许

取消

允许

×

分析

微信扫一扫可打开此内容，使用完整服务

老码小张已关注赞分享推荐 写留言

：

，

，

，

，

，

，

，

，

，

，

，

，

。

视频

小程序

赞

，轻点两下取消赞

在看

，轻点两下取消在看

分享

留言

收藏

听过

可在「公众号 > 右上角  > 划线」找到划线过的内容我知道了,,选择留言身份留言暂无留言1条留言已无更多数据发消息   写留言:关闭更多关闭确认提交投诉你可以补充投诉原因（选填）确定
