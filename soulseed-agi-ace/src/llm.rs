use reqwest::blocking::Client;
use serde::Serialize;

use crate::errors::AceError;
use soulseed_agi_core_models::awareness::{ClarifyPlan, SelfPlan};

#[derive(Clone)]
pub struct OpenAiClient {
    api_key: String,
    model: String,
    client: Client,
}

impl OpenAiClient {
    pub fn from_env() -> Option<Result<Self, AceError>> {
        let api_key = std::env::var("OPENAI_API_KEY").ok()?;
        let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
        let client = Client::builder()
            .build()
            .map_err(|err| AceError::ThinWaist(format!("openai client init failed: {err}")));
        Some(client.map(|client| Self {
            api_key,
            model,
            client,
        }))
    }

    pub fn clarify_answer(
        &self,
        plan: &ClarifyPlan,
        context: &str,
    ) -> Result<String, AceError> {
        let mut questions = String::new();
        for (idx, q) in plan.questions.iter().enumerate() {
            questions.push_str(&format!("{}. {}\n", idx + 1, q.text));
        }
        let user_prompt = format!(
            "用户输入：\n{}\n\n请逐条回答以下澄清问题，使用中文，保持简洁：\n{}",
            context, questions
        );
        self.chat_completion("你是一名智能澄清助手，需针对用户问题逐条给出明确回答。", &user_prompt, 512)
    }

    pub fn self_reflection(
        &self,
        plan: &SelfPlan,
        context: &str,
    ) -> Result<String, AceError> {
        let hint = plan
            .hint
            .clone()
            .unwrap_or_else(|| "总结核心要点并给出下一步推理建议。".into());
        let user_prompt = format!(
            "上下文：{}\n\n请按照提示进行自我反思：{}",
            context, hint
        );
        self.chat_completion(
            "你是 Soulseed AGI 的自省模块，请输出结构化自省结论，包含问题洞察与下一步建议。",
            &user_prompt,
            512,
        )
    }

    fn chat_completion(
        &self,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: u16,
    ) -> Result<String, AceError> {
        let body = OpenAiChatRequest {
            model: &self.model,
            messages: &[
                Message {
                    role: "system",
                    content: system_prompt,
                },
                Message {
                    role: "user",
                    content: user_prompt,
                },
            ],
            max_tokens,
            temperature: 0.2,
        };

        let response = self
            .client
            .post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .map_err(|err| AceError::ThinWaist(format!("openai request failed: {err}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response
                .text()
                .unwrap_or_else(|_| "<no body>".into());
            return Err(AceError::ThinWaist(format!(
                "openai error {status}: {text}"
            )));
        }

        let completion: OpenAiChatResponse = response
            .json()
            .map_err(|err| AceError::ThinWaist(format!("parse openai response failed: {err}")))?;

        completion
            .choices
            .into_iter()
            .find_map(|c| c.message.content)
            .ok_or_else(|| AceError::ThinWaist("openai response missing content".into()))
    }
}

#[derive(Serialize)]
struct OpenAiChatRequest<'a> {
    model: &'a str,
    messages: &'a [Message<'a>],
    max_tokens: u16,
    temperature: f32,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(serde::Deserialize)]
struct OpenAiChatResponse {
    choices: Vec<Choice>,
}

#[derive(serde::Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(serde::Deserialize)]
struct ChoiceMessage {
    content: Option<String>,
}
