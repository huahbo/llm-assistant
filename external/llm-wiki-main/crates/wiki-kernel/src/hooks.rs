use wiki_core::WikiEvent;

/// 事件钩子：可接到 outbox、日志、或未来的 `rust-mempalace` 同步。
pub trait WikiHook: Send {
    fn on_event(&mut self, event: &WikiEvent);
}

#[derive(Debug, Default)]
pub struct NoopWikiHook;

impl WikiHook for NoopWikiHook {
    fn on_event(&mut self, _event: &WikiEvent) {}
}
