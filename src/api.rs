use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Clone)]
pub struct OllamaClient {
    api_url: String,
    model: String,
    client: Client,
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
    pub fn new(api_url: &str, model: &str) -> Self {
        Self {
            api_url: api_url.to_string(),
            model: model.to_string(),
            client: Client::new(),
        }
    }

    pub async fn generate_response(
        &self,
        prompt: &str,
        context: &str,
        conversation_history: &[String],
    ) -> Result<String> {
        let history = conversation_history.join("\n");
        
        let system_prompt = format!(
            "You are a helpful assistant for software development. \
            You can provide code suggestions and explanations. \
            When suggesting changes to code, use markdown code blocks and \
            show diffs in the format of: \
            ```diff\npath/to/file.ext\n- old line\n+ new line\n```"
        );
        
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

        let response = self.client
            .post(&request_url)
            .json(&request_body)
            .send()
            .await?
            .json::<OllamaResponse>()
            .await?;

        Ok(response.response)
    }
}