---
type: clip
title: "coder-brzhang/llm-wiki: llm-wiki 是一个“持久、可累积”的 LLM 知识库内核与最小 CLI：不是每次 query 现做 RAG，而是把知识沉淀为可维护的结构化状态，并可投影成可浏览的 Markdown wiki（适配 Obsidian 等）。"
url: "https://github.com/coder-brzhang/llm-wiki"
clipped: 2026-04-22
origin: clipper
tags: [clip]
---

# coder-brzhang/llm-wiki: llm-wiki 是一个“持久、可累积”的 LLM 知识库内核与最小 CLI：不是每次 query 现做 RAG，而是把知识沉淀为可维护的结构化状态，并可投影成可浏览的 Markdown wiki（适配 Obsidian 等）。

Source: https://github.com/coder-brzhang/llm-wiki

Skip to content

Search code, repositories, users, issues, pull requests...

Search

Clear

0 suggestions.

Search syntax tips

Give feedback

Provide feedback

We read every piece of feedback, and take your input very seriously.

Include my email address so I can be contacted

Cancel

Submit feedback

Saved searches

Use saved searches to filter your results more quickly

Name

Query

To see all available qualifiers, see our documentation.

Cancel

Create saved search

You signed in with another tab or window. Reload to refresh your session.

You signed out in another tab or window. Reload to refresh your session.

You switched accounts on another tab or window. Reload to refresh your session.

Dismiss alert

Open in github.dev

Open in a new github.dev tab

Open in codespace

main1 Branch0 Tags tTGo to fileAdd fileAdd fileCodeOpen more actions menuFolders and filesNameNameLast commit messageLast commit dateLatest commitbravekingzhangAdd rust-llm-wiki startup banner and print before CLI parseOpen commit detailsApr 17, 2026d2cae39 · Apr 17, 2026History2 CommitsOpen commit details2 CommitscratescratesAdd rust-llm-wiki startup banner and print before CLI parseApr 17, 2026docsdocsBootstrap llm-wiki with core engine, CLI workflows, and end-to-end te…Apr 16, 2026scriptsscriptsBootstrap llm-wiki with core engine, CLI workflows, and end-to-end te…Apr 16, 2026.gitignore.gitignoreAdd rust-llm-wiki startup banner and print before CLI parseApr 17, 2026AGENTS.mdAGENTS.mdBootstrap llm-wiki with core engine, CLI workflows, and end-to-end te…Apr 16, 2026Cargo.lockCargo.lockBootstrap llm-wiki with core engine, CLI workflows, and end-to-end te…Apr 16, 2026Cargo.tomlCargo.tomlBootstrap llm-wiki with core engine, CLI workflows, and end-to-end te…Apr 16, 2026README.mdREADME.mdAdd rust-llm-wiki startup banner and print before CLI parseApr 17, 2026llm-config.example.tomlllm-config.example.tomlBootstrap llm-wiki with core engine, CLI workflows, and end-to-end te…Apr 16, 2026View all filesRepository files navigationEdit filellm-wiki

llm-wiki 是一个“持久、可累积”的 LLM 知识库内核与最小 CLI：不是每次 query 现做 RAG，而是把知识沉淀为可维护的结构化状态，并可投影成可浏览的 Markdown wiki（适配 Obsidian 等）。

核心理念（对齐 Karpathy LLM Wiki）

Raw sources：不可变的原始资料（RawArtifact）。

Wiki：可持续维护的 wiki 页面（WikiPage，markdown + [[wikilink]]）。

Schema：约束与策略（DomainSchema：允许的实体/关系、质量阈值、保留/晋升参数）。

Operations：ingest / query / lint / crystallize，并通过 outbox 事件让外部 consumer（如 mempalace）接入。

相对 idea-only 的方案，这个仓库更偏“工程内核”：事件、审计、生命周期、RRF 融合检索、保留强度加权、outbox 语义等都已最小落地。

快速开始

1) 运行一次 ingest（并同步 markdown wiki 投影）

cargo run -p wiki-cli -- \

--db wiki.db \

--wiki-dir wiki \

--sync-wiki \

ingest "file:///notes/a.md" "项目使用 Redis\nAuthorization: Bearer secret" \

--scope private:cli

会生成/更新：

wiki/index.md

wiki/log.md

wiki/pages/、wiki/concepts/、wiki/sources/

2) query（可选落盘为 wiki 页面）

cargo run -p wiki-cli -- \

--db wiki.db \

--wiki-dir wiki \

--sync-wiki \

query "Redis API" --write-page --page-title "analysis-redis-api"

3) lint（并输出报告）

cargo run -p wiki-cli -- \

--db wiki.db \

--wiki-dir wiki \

--sync-wiki \

lint

会写入 wiki/reports/lint-*.md，并在 stdout 打印报告路径。

4) outbox 增量导出与消费确认

增量导出（offset 模式）：

cargo run -p wiki-cli -- --db wiki.db export-outbox-ndjson-from --last-id 100

消费确认（标记 processed）：

cargo run -p wiki-cli -- --db wiki.db ack-outbox --up-to-id 120 --consumer-tag mempalace

5) mempalace（最小桥接消费演示）

cargo run -p wiki-cli -- --db wiki.db consume-to-mempalace --last-id 100

当前实现为“打印型 sink”，用于验证 outbox 消费链路与事件映射；后续可替换为真实 mempalace 写入实现。

模型配置（由你填写）

当前代码库的核心能力不依赖模型调用；如果你要接入 DeepSeek（或其它 OpenAI-compatible API），可先填写模板文件：

llm-config.example.toml：复制为 llm-config.toml 后填写 base_url / api_key / model

注意：不要提交真实 api_key 到 git。

测试

cargo test

测试覆盖：

wiki 投影输出（index.md/log.md 与目录结构）

outbox 游标导出与 ack

mempalace bridge 的 NDJSON 消费分发

端到端回归（推荐）

./scripts/e2e.sh

该脚本会自动执行并断言：

ingest / file-claim / supersede-claim / query / lint 全链路

outbox 增量导出与 ack

mempalace 消费结果 consumed > 0

若存在 llm-config.toml，自动执行 llm-smoke（DeepSeek 冒烟）

文档

AGENTS.md：面向 agent 的稳定工作流规范

docs/plan.md：里程碑与验收标准

About

llm-wiki 是一个“持久、可累积”的 LLM 知识库内核与最小 CLI：不是每次 query 现做 RAG，而是把知识沉淀为可维护的结构化状态，并可投影成可浏览的 Markdown wiki（适配 Obsidian 等）。

Resources

Readme

Uh oh!

There was an error while loading. Please reload this page.

Activity

Custom properties

Stars

0

stars

Watchers

0

watching

Forks

0

forks

Releases

No releases published

Packages

0

No packages published

Contributors

1

bravekingzhang

brzhang

Languages

Rust

97.4%

Shell

2.6%

You can’t perform that action at this time.
