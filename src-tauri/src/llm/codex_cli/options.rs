use super::{invalid, LLMError, ModelInfo, Server};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelOptions {
    pub model: String,
    pub efforts: Vec<String>,
    pub initial_effort: String,
    pub fast_tier: Option<String>,
}

impl ModelOptions {
    fn from_catalog(model: &str, item: &Value) -> Self {
        let efforts: Vec<String> = item["supportedReasoningEfforts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| e["reasoningEffort"].as_str().map(String::from))
            .collect();
        // Keep the existing low setting on models that offer it.
        let initial_effort = if efforts.iter().any(|e| e == "low") {
            "low"
        } else {
            item["defaultReasoningEffort"].as_str().unwrap_or("low")
        };
        let fast_tier = item["serviceTiers"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|t| {
                t["id"] == "priority"
                    || t["name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case("fast"))
            })
            .and_then(|t| t["id"].as_str())
            .map(String::from)
            .or_else(|| {
                item["additionalSpeedTiers"]
                    .as_array()?
                    .iter()
                    .any(|v| v == "fast")
                    .then(|| "priority".to_owned())
            });
        Self {
            model: model.to_owned(),
            efforts,
            initial_effort: initial_effort.into(),
            fast_tier,
        }
    }

    pub(super) fn settings(
        &self,
        effort: Option<&str>,
        speed: Option<&str>,
    ) -> Result<(&str, Option<&str>), LLMError> {
        let effort = match effort {
            None => self.initial_effort.as_str(),
            Some(value) => self.efforts.iter().find(|e| e.as_str() == value).map(String::as_str)
                .ok_or_else(|| LLMError::NotConfigured(format!("{} does not support reasoning effort '{value}'. Update Codex CLI settings.",self.model)))?,
        };
        let tier = match speed.unwrap_or("inherit") {
            "inherit" => None,
            "standard" => Some("default"),
            "fast" => Some(self.fast_tier.as_deref().ok_or_else(|| {
                LLMError::NotConfigured(format!(
                    "Fast is unavailable for {}. Choose Standard or Codex default.",
                    self.model
                ))
            })?),
            other => {
                return Err(LLMError::NotConfigured(format!(
                    "Unknown Codex speed '{other}'"
                )))
            }
        };
        Ok((effort, tier))
    }
}

impl Server {
    pub(super) async fn with_models() -> Result<(Self, Vec<ModelInfo>), LLMError> {
        let mut server = Self::connect().await?;
        let config = server
            .call(
                "config/read",
                json!({"includeLayers":false,"cwd":super::sandbox_dir()}),
            )
            .await?;
        let configured = config.pointer("/config/model").and_then(Value::as_str);
        let mut catalog_default = None;
        let mut models = Vec::new();
        let mut cursor = Value::Null;
        loop {
            let page = server
                .call("model/list", json!({"cursor":cursor,"limit":100}))
                .await?;
            for item in page["data"]
                .as_array()
                .ok_or_else(|| invalid("missing model catalog"))?
            {
                let Some(model) = item["model"].as_str() else {
                    continue;
                };
                if item["isDefault"] == true {
                    catalog_default = Some(model.to_owned());
                }
                models.push(ModelInfo {
                    id: model.to_owned(),
                    name: item["displayName"].as_str().unwrap_or(model).to_owned(),
                    provider: "codex_cli".into(),
                    context_window: None,
                    codex: Some(ModelOptions::from_catalog(model, item)),
                });
            }
            cursor = page["nextCursor"].clone();
            if cursor.is_null() {
                break;
            }
        }
        // The user's configured model can differ from the catalog's isDefault.
        let resolved = configured.or(catalog_default.as_deref());
        let options = models
            .iter()
            .find(|m| Some(m.id.as_str()) == resolved)
            .and_then(|m| m.codex.clone());
        models.insert(
            0,
            ModelInfo {
                id: "default".into(),
                name: resolved
                    .map(|id| format!("Codex default ({id})"))
                    .unwrap_or_else(|| "Codex default".into()),
                provider: "codex_cli".into(),
                context_window: None,
                codex: options,
            },
        );
        Ok((server, models))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherited_standard_and_fast_are_distinct() {
        let model = ModelOptions::from_catalog(
            "m",
            &json!({
                "supportedReasoningEfforts":[{"reasoningEffort":"high"},{"reasoningEffort":"low"}],
                "defaultReasoningEffort":"high","serviceTiers":[{"id":"priority","name":"Fast"}]
            }),
        );
        assert_eq!(model.settings(None, None).unwrap(), ("low", None));
        assert_eq!(
            model.settings(Some("high"), Some("standard")).unwrap(),
            ("high", Some("default"))
        );
        assert_eq!(
            model.settings(Some("high"), Some("fast")).unwrap(),
            ("high", Some("priority"))
        );
    }

    #[test]
    fn rejects_saved_choices_that_the_model_no_longer_offers() {
        let model = ModelOptions::from_catalog(
            "m",
            &json!({
                "supportedReasoningEfforts":[{"reasoningEffort":"high"}],"defaultReasoningEffort":"high"
            }),
        );
        assert_eq!(model.settings(None, None).unwrap(), ("high", None));
        assert!(model.settings(Some("low"), None).is_err());
        assert!(model.settings(None, Some("fast")).is_err());
    }
}
