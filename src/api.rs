use anyhow::{Result, anyhow};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct OllamaClient {
    api_url: String,
    model: String,
    client: Client,
    config: Config,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    system: String,
    stream: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaResponse {
    model: String,
    response: String,
}

impl OllamaClient {
    pub fn new(api_url: &str, model: &str, config: Config) -> Self {
        Self {
            api_url: api_url.to_string(),
            model: model.to_string(),
            client: Client::new(),
            config,
        }
    }
    
    pub fn get_api_url(&self) -> &str {
        &self.api_url
    }
    
    /// Tests if the connection to Ollama is working
    pub async fn test_connection(&self) -> Result<bool> {
        let request_url = format!("{}/api/tags", self.api_url);
        
        let response = self.client
            .get(&request_url)
            .send()
            .await;
            
        match response {
            Ok(res) => Ok(res.status().is_success()),
            Err(_) => Ok(false)
        }
    }
    
    /// Gets a list of available models from Ollama
    pub async fn get_available_models(&self) -> Result<Vec<String>> {
        let request_url = format!("{}/api/tags", self.api_url);
        
        let response = self.client
            .get(&request_url)
            .send()
            .await?;
            
        if !response.status().is_success() {
            return Err(anyhow!("Failed to get available models, status: {}", response.status()));
        }
        
        let body = response.text().await?;
        let json: Value = serde_json::from_str(&body)?;
        
        // Parse the JSON response to extract model names
        let models = match json.get("models") {
            Some(models_array) => {
                let mut model_names = HashSet::new();
                
                if let Some(array) = models_array.as_array() {
                    for model_obj in array {
                        if let Some(name) = model_obj.get("name").and_then(|n| n.as_str()) {
                            model_names.insert(name.to_string());
                        }
                    }
                }
                
                // Convert to sorted Vec
                let mut model_names_vec: Vec<String> = model_names.into_iter().collect();
                model_names_vec.sort();
                model_names_vec
            },
            None => Vec::new(),
        };
        
        Ok(models)
    }

    pub async fn generate_response(
        &self,
        prompt: &str,
        context: &str,
        conversation_history: &[String],
    ) -> Result<String> {
        let history = conversation_history.join("\n");
        
        // Get the configured system prompt for this model
        let system_prompt = self.config.get_system_prompt(&self.model);
        
        let full_prompt = format!(
            "{}\n\nContext of the current directory:\n{}\n\nUser request: {}",
            history, context, prompt
        );

        let request_url = format!("{}/api/generate", self.api_url);
        
        let request_body = json!({
            "model": self.model,
            "prompt": full_prompt,
            "system": system_prompt,
            "stream": false
        });

        let raw_response = self.client
            .post(&request_url)
            .json(&request_body)
            .send()
            .await?;
            
        // Store status and raw text for debugging purposes
        let status = raw_response.status();
        let body = raw_response.text().await?;
        
        // Try to deserialize
        match serde_json::from_str::<OllamaResponse>(&body) {
            Ok(parsed) => Ok(parsed.response),
            Err(e) => {
                // Include meaningful error that shows what's happening
                let err_msg = format!(
                    "Failed to parse response (Status: {}): {} \nRequest URL: {}\nRaw response: {}", 
                    status, e, request_url, body
                );
                Err(anyhow::anyhow!(err_msg))
            }
        }
    }
}