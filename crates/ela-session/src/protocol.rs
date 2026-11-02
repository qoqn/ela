use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use ela_core::ResolvedConfig;
use serde::{Deserialize, Serialize};

use crate::{Session, SessionError, SessionSink};

/// The kind of view the UI uses to display a Session. The frontend branches on
/// this, never on Protocol IDs (ADR-0003).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RendererKind {
    Terminal,
    Framebuffer,
    FileBrowser,
    Web,
    External,
}

#[derive(Debug, Clone, Copy, Default, Serialize, specta::Type)]
pub struct Capabilities {
    pub resize: bool,
    pub clipboard: bool,
    pub file_transfer: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ProtocolInfo {
    pub id: &'static str,
    pub display_name: &'static str,
    pub default_port: Option<u16>,
    pub renderer: RendererKind,
    pub capabilities: Capabilities,
    pub settings_schema: serde_json::Value,
    pub listed: bool,
}

/// Checks a Connection's options against a `settings_schema`. Keys the schema doesn't know are
/// left alone: they may come from a newer Ela, or belong to another Protocol.
pub fn check_options(
    schema: &serde_json::Value,
    options: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    for (key, value) in options {
        let Some(field) = schema.get(key) else {
            continue;
        };
        let label = field
            .get("label")
            .and_then(|label| label.as_str())
            .unwrap_or(key);
        let fits = match field.get("type").and_then(|kind| kind.as_str()) {
            Some("string") => value.is_string(),
            Some("integer") => value.is_i64() || value.is_u64(),
            // Text is a list Ela wrote before lists were arrays.
            Some("list") => {
                value.is_string()
                    || value
                        .as_array()
                        .is_some_and(|items| items.iter().all(|item| item.is_string()))
            }
            Some("enum") => field
                .get("values")
                .and_then(|values| values.as_array())
                .is_some_and(|values| values.contains(value)),
            _ => true,
        };
        if !fits {
            let value = value
                .as_str()
                .map_or_else(|| value.to_string(), |text| format!("“{text}”"));
            return Err(format!("{label} can't be {value}."));
        }
    }
    Ok(())
}

#[async_trait]
pub trait Protocol: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn default_port(&self) -> Option<u16>;
    fn renderer(&self) -> RendererKind;

    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }

    fn settings_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }

    /// Whether people can save Connections with it. Measuring tools say no.
    fn listed(&self) -> bool {
        true
    }

    async fn connect(
        &self,
        config: ResolvedConfig,
        sink: &SessionSink,
    ) -> Result<Box<dyn Session>, SessionError>;

    fn info(&self) -> ProtocolInfo {
        ProtocolInfo {
            id: self.id(),
            display_name: self.display_name(),
            default_port: self.default_port(),
            renderer: self.renderer(),
            capabilities: self.capabilities(),
            settings_schema: self.settings_schema(),
            listed: self.listed(),
        }
    }
}

#[derive(Default, Clone)]
pub struct ProtocolRegistry {
    protocols: BTreeMap<&'static str, Arc<dyn Protocol>>,
}

impl ProtocolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, protocol: impl Protocol) -> &mut Self {
        self.protocols.insert(protocol.id(), Arc::new(protocol));
        self
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Protocol>> {
        self.protocols.get(id).cloned()
    }

    pub fn list(&self) -> Vec<ProtocolInfo> {
        self.protocols.values().map(|p| p.info()).collect()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn check(options: serde_json::Value) -> Result<(), String> {
        let schema = json!({
            "term": { "type": "string" },
            "fps": { "type": "integer", "label": "Frames per second" },
            "quality": { "type": "enum", "values": ["auto", "lossless"] },
            "keys": { "type": "list", "label": "Identity files" },
        });
        check_options(&schema, options.as_object().unwrap())
    }

    #[test]
    fn options_that_fit_the_schema_pass() {
        assert_eq!(
            check(json!({ "term": "xterm", "fps": 60, "quality": "lossless" })),
            Ok(())
        );
    }

    #[test]
    fn options_the_schema_does_not_know_pass() {
        assert_eq!(check(json!({ "from_elsewhere": [1, 2] })), Ok(()));
    }

    #[test]
    fn options_of_the_wrong_kind_are_refused_by_their_label() {
        assert_eq!(
            check(json!({ "fps": "fast" })),
            Err("Frames per second can't be “fast”.".into())
        );
        assert!(check(json!({ "term": 5 })).is_err());
        assert!(check(json!({ "quality": "potato" })).is_err());
    }

    #[test]
    fn a_list_is_strings_or_the_older_comma_text() {
        assert_eq!(check(json!({ "keys": ["~/.ssh/a,b", "/c"] })), Ok(()));
        assert_eq!(check(json!({ "keys": "~/.ssh/a, /c" })), Ok(()));
        assert!(check(json!({ "keys": ["~/.ssh/a", 5] })).is_err());
    }
}
