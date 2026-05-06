//! Tool registry. Section 11.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::result::ToolResult;

/// A single named tool the runtime can call.
#[async_trait]
pub trait Tool: Send + Sync + 'static {
    fn name(&self) -> &str;

    /// Tool category — used to classify scope groups.
    fn category(&self) -> ToolCategory {
        ToolCategory::ReadOnly
    }

    async fn invoke(&self, args: Value) -> ToolResult;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolCategory {
    ReadOnly,
    SafeWrite,
    DangerousWrite,
    External,
}

pub type ToolBox = Arc<dyn Tool>;

#[derive(Default, Clone)]
pub struct ToolRegistry {
    tools: HashMap<String, ToolBox>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: ToolBox) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&ToolBox> {
        self.tools.get(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Convenience: register a closure-based tool. Useful for tests.
    pub fn register_fn<F, Fut>(&mut self, name: &'static str, f: F)
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = ToolResult> + Send + 'static,
    {
        struct ClosureTool<F> {
            name: &'static str,
            f: F,
        }
        #[async_trait]
        impl<F, Fut> Tool for ClosureTool<F>
        where
            F: Fn(Value) -> Fut + Send + Sync + 'static,
            Fut: std::future::Future<Output = ToolResult> + Send + 'static,
        {
            fn name(&self) -> &str {
                self.name
            }
            async fn invoke(&self, args: Value) -> ToolResult {
                (self.f)(args).await
            }
        }
        self.register(Arc::new(ClosureTool { name, f }));
    }
}
