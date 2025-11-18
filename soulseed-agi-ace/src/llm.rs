use soulbase_llm::{
    chat::{ChatRequest, ResponseFormat, ResponseKind},
    jsonsafe::StructOutPolicy,
    model::{ContentSegment, Message, Role},
    provider::Registry,
};

use soulbase_llm::provider::OpenAiProviderFactory;
use soulbase_llm::provider::ClaudeProviderFactory;
use soulbase_llm::provider::GeminiProviderFactory;

use crate::errors::AceError;
use soulseed_agi_core_models::awareness::{ClarifyPlan, SelfPlan};

/// ACE 的统一 LLM 客户端，基于 soul-base 的 LLM 抽象层
#[derive(Clone)]
pub struct AceLlmClient {
    registry: std::sync::Arc<Registry>,
    clarify_model: String,
    reflect_model: String,
}

impl AceLlmClient {
    /// 从环境变量初始化 LLM 客户端
    ///
    /// 环境变量：
    /// - ACE_LLM_PROVIDER: 默认 provider (如 openai, claude, gemini)
    /// - ACE_LLM_CLARIFY_MODEL: 澄清功能使用的模型 (默认使用 provider:gpt-4o-mini 或 provider:claude-3-haiku)
    /// - ACE_LLM_REFLECT_MODEL: 自我反思功能使用的模型 (默认同 clarify)
    /// - OPENAI_API_KEY, ANTHROPIC_API_KEY, GEMINI_API_KEY: 各 provider 的 API key
    pub fn from_env() -> Option<Result<Self, AceError>> {
        let provider = std::env::var("ACE_LLM_PROVIDER").ok()?;

        // 构建模型 ID
        let default_model = Self::default_model_for_provider(&provider);
        let clarify_model = std::env::var("ACE_LLM_CLARIFY_MODEL")
            .unwrap_or_else(|_| default_model.clone());
        let reflect_model = std::env::var("ACE_LLM_REFLECT_MODEL")
            .unwrap_or_else(|_| clarify_model.clone());

        // 初始化 registry 并注册 providers
        let mut registry = Registry::new();

        if let Some(api_key) = std::env::var("OPENAI_API_KEY").ok() {
            if let Ok(config) = soulbase_llm::provider::OpenAiConfig::new(&api_key) {
                if let Ok(factory) = OpenAiProviderFactory::new(config) {
                    registry.register(Box::new(factory));
                }
            }
        }

        if let Some(api_key) = std::env::var("ANTHROPIC_API_KEY").ok() {
            if let Ok(config) = soulbase_llm::provider::ClaudeConfig::new(&api_key) {
                if let Ok(factory) = ClaudeProviderFactory::new(config) {
                    registry.register(Box::new(factory));
                }
            }
        }

        if let Some(api_key) = std::env::var("GEMINI_API_KEY").ok() {
            if let Ok(config) = soulbase_llm::provider::GeminiConfig::new(&api_key) {
                if let Ok(factory) = GeminiProviderFactory::new(config) {
                    registry.register(Box::new(factory));
                }
            }
        }

        Some(Ok(Self {
            registry: std::sync::Arc::new(registry),
            clarify_model,
            reflect_model,
        }))
    }

    fn default_model_for_provider(provider: &str) -> String {
        match provider {
            "openai" => "openai:gpt-4o-mini".to_string(),
            "claude" => "claude:claude-3-haiku".to_string(),
            "gemini" => "gemini:gemini-pro".to_string(),
            _ => format!("{}:default", provider),
        }
    }

    /// 澄清功能：根据 ClarifyPlan 生成答案
    pub async fn clarify_answer(&self, plan: &ClarifyPlan, context: &str) -> Result<String, AceError> {
        let mut questions = String::new();
        for (idx, q) in plan.questions.iter().enumerate() {
            questions.push_str(&format!("{}. {}\n", idx + 1, q.text));
        }
        let user_prompt = format!(
            "用户输入：\n{}\n\n请逐条回答以下澄清问题，使用中文，保持简洁：\n{}",
            context, questions
        );

        self.chat_completion(
            &self.clarify_model,
            "你是一名智能澄清助手，需针对用户问题逐条给出明确回答。",
            &user_prompt,
            512,
        ).await
    }

    /// 自我反思功能：根据 SelfPlan 生成反思内容
    pub async fn self_reflection(&self, plan: &SelfPlan, context: &str) -> Result<String, AceError> {
        let hint = plan.hint.clone()
            .unwrap_or_else(|| "总结核心要点并给出下一步推理建议。".into());
        let user_prompt = format!("上下文：{}\n\n请按照提示进行自我反思：{}", context, hint);

        self.chat_completion(
            &self.reflect_model,
            "你是 Soulseed AGI 的自省模块，请输出结构化自省结论，包含问题洞察与下一步建议。",
            &user_prompt,
            512,
        ).await
    }

    async fn chat_completion(
        &self,
        model_id: &str,
        system_prompt: &str,
        user_prompt: &str,
        max_tokens: u32,
    ) -> Result<String, AceError> {
        // 获取 ChatModel
        let chat_model = self.registry
            .chat(model_id)
            .ok_or_else(|| AceError::ThinWaist(format!("模型不可用: {}", model_id)))?;

        // 构建消息
        let messages = vec![
            Message {
                role: Role::System,
                segments: vec![ContentSegment::Text {
                    text: system_prompt.to_string(),
                }],
                tool_calls: vec![],
            },
            Message {
                role: Role::User,
                segments: vec![ContentSegment::Text {
                    text: user_prompt.to_string(),
                }],
                tool_calls: vec![],
            },
        ];

        // 构建请求
        let request = ChatRequest {
            model_id: model_id.to_string(),
            messages,
            tool_specs: vec![],
            temperature: Some(0.2),
            max_tokens: Some(max_tokens),
            top_p: None,
            stop: vec![],
            seed: None,
            frequency_penalty: None,
            presence_penalty: None,
            logit_bias: serde_json::Map::new(),
            response_format: Some(ResponseFormat {
                kind: ResponseKind::Text,
                json_schema: None,
                strict: false,
            }),
            idempotency_key: None,
            cache_hint: None,
            allow_sensitive: false,
            metadata: serde_json::Value::Null,
        };

        // 执行聊天完成
        let response = chat_model
            .chat(request, &StructOutPolicy::Off)
            .await
            .map_err(|e| AceError::ThinWaist(format!("LLM 调用失败: {}", e)))?;

        // 提取文本内容
        response.message
            .segments
            .into_iter()
            .find_map(|seg| {
                if let ContentSegment::Text { text } = seg {
                    Some(text)
                } else {
                    None
                }
            })
            .ok_or_else(|| AceError::ThinWaist("LLM 响应缺少文本内容".into()))
    }
}
