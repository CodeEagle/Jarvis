//! MCP-style plugin registry (Section 24.5).
//!
//! A "plugin" packages a set of tools that the runtime can register
//! together. Each plugin owns a stable id, a human label, and a
//! constructor that returns the `ToolBox` instances to add. Plugins
//! that fail to load are surfaced as errors rather than panics, and
//! the registry tracks which tool came from which plugin so the
//! runtime can list / unload them later.
//!
//! We don't speak the wire-level Model Context Protocol here — that
//! belongs in a dedicated network adapter crate. This module gives
//! us the *shape* MCP needs: a stable plugin trait, lifecycle
//! management, and provenance tracking on tools.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;

use crate::registry::{ToolBox, ToolRegistry};

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin '{0}' is already registered")]
    AlreadyRegistered(String),
    #[error("plugin '{0}' is not registered")]
    NotRegistered(String),
    #[error("plugin '{plugin}' load failed: {reason}")]
    Load { plugin: String, reason: String },
}

#[async_trait]
pub trait Plugin: Send + Sync {
    fn id(&self) -> &str;
    fn label(&self) -> &str;
    /// Return the tools this plugin contributes. Called once at
    /// registration time. Implementors that need network / FS access
    /// should set up resources here and surface errors via
    /// `PluginError::Load`.
    async fn build_tools(&self) -> Result<Vec<ToolBox>, PluginError>;
}

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub id: String,
    pub label: String,
    pub tool_names: Vec<String>,
}

#[derive(Default)]
pub struct PluginRegistry {
    /// Tool name → plugin id, for unload provenance.
    provenance: RwLock<HashMap<String, String>>,
    /// Plugin id → metadata.
    plugins: RwLock<HashMap<String, LoadedPlugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Install a plugin and contribute its tools to the supplied
    /// `ToolRegistry`. Fails if a plugin with the same id is already
    /// registered or if the plugin's `build_tools` errors.
    pub async fn install(
        &self,
        plugin: Arc<dyn Plugin>,
        tools: &mut ToolRegistry,
    ) -> Result<LoadedPlugin, PluginError> {
        let id = plugin.id().to_string();
        if self.plugins.read().expect("plugin lock").contains_key(&id) {
            return Err(PluginError::AlreadyRegistered(id));
        }
        let built = plugin.build_tools().await?;
        let mut tool_names = Vec::with_capacity(built.len());
        for tool in built {
            let name = tool.name().to_string();
            tools.register(tool);
            tool_names.push(name.clone());
            self.provenance
                .write()
                .expect("plugin lock")
                .insert(name, id.clone());
        }
        let entry = LoadedPlugin {
            id: id.clone(),
            label: plugin.label().to_string(),
            tool_names,
        };
        self.plugins
            .write()
            .expect("plugin lock")
            .insert(id, entry.clone());
        Ok(entry)
    }

    /// Forget a plugin's tools — used when the plugin is being
    /// unloaded. Removes the provenance entries; the caller is
    /// expected to clear the corresponding tools from its
    /// `ToolRegistry` (the registry currently doesn't expose remove,
    /// so this method tracks the names that need clearing).
    pub fn unregister(&self, plugin_id: &str) -> Result<Vec<String>, PluginError> {
        let mut plugins = self.plugins.write().expect("plugin lock");
        let entry = plugins
            .remove(plugin_id)
            .ok_or_else(|| PluginError::NotRegistered(plugin_id.to_string()))?;
        let mut prov = self.provenance.write().expect("plugin lock");
        for name in &entry.tool_names {
            prov.remove(name);
        }
        Ok(entry.tool_names)
    }

    pub fn list(&self) -> Vec<LoadedPlugin> {
        self.plugins
            .read()
            .expect("plugin lock")
            .values()
            .cloned()
            .collect()
    }

    pub fn provenance_of(&self, tool_name: &str) -> Option<String> {
        self.provenance
            .read()
            .expect("plugin lock")
            .get(tool_name)
            .cloned()
    }
}
