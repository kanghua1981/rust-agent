//! Shared LLM response types.
//!
//! [`crate::streaming`] is the single model-call path (streaming); this module
//! holds the response types it returns.

use serde::{Deserialize, Serialize};

/// Response from the LLM
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub content: Vec<crate::conversation::ContentBlock>,
    pub stop_reason: Option<String>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}
