use crate::session::Session;

// Money is INTEGER (operator rule: no float). Unit: NANO-USD (1e-9 USD) per million tokens.
// $15.00 -> 15_000_000_000 nano-USD. Exact at every arithmetic step; no rounding drift.
const DEFAULT_INPUT_COST_PER_MILLION_NANOUSD: u64 = 15_000_000_000;
const DEFAULT_OUTPUT_COST_PER_MILLION_NANOUSD: u64 = 75_000_000_000;
const DEFAULT_CACHE_CREATION_COST_PER_MILLION_NANOUSD: u64 = 18_750_000_000;
const DEFAULT_CACHE_READ_COST_PER_MILLION_NANOUSD: u64 = 1_500_000_000;

/// Per-million-token pricing used for cost estimation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelPricing {
    /// All fields are NANO-USD (1e-9 USD) per million tokens; integer only.
    pub input_cost_per_million_nanousd: u64,
    pub output_cost_per_million_nanousd: u64,
    pub cache_creation_cost_per_million_nanousd: u64,
    pub cache_read_cost_per_million_nanousd: u64,
}

impl ModelPricing {
    #[must_use]
    pub const fn default_sonnet_tier() -> Self {
        Self {
            input_cost_per_million_nanousd: DEFAULT_INPUT_COST_PER_MILLION_NANOUSD,
            output_cost_per_million_nanousd: DEFAULT_OUTPUT_COST_PER_MILLION_NANOUSD,
            cache_creation_cost_per_million_nanousd: DEFAULT_CACHE_CREATION_COST_PER_MILLION_NANOUSD,
            cache_read_cost_per_million_nanousd: DEFAULT_CACHE_READ_COST_PER_MILLION_NANOUSD,
        }
    }
}

/// Token counters accumulated for a conversation turn or session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_creation_input_tokens: u32,
    pub cache_read_input_tokens: u32,
}

/// Estimated dollar cost derived from a [`TokenUsage`] sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UsageCostEstimate {
    /// All fields are NANO-USD (1e-9 USD); integer only.
    pub input_cost_nanousd: u64,
    pub output_cost_nanousd: u64,
    pub cache_creation_cost_nanousd: u64,
    pub cache_read_cost_nanousd: u64,
}

impl UsageCostEstimate {
    #[must_use]
    pub const fn total_cost_nanousd(self) -> u64 {
        self.input_cost_nanousd
            + self.output_cost_nanousd
            + self.cache_creation_cost_nanousd
            + self.cache_read_cost_nanousd
    }
}

/// Returns pricing metadata for a known model alias or family.
#[must_use]
pub fn pricing_for_model(model: &str) -> Option<ModelPricing> {
    let normalized = model.to_ascii_lowercase();
    if normalized.contains("haiku") {
        return Some(ModelPricing {
            input_cost_per_million_nanousd: 1_000_000_000,
            output_cost_per_million_nanousd: 5_000_000_000,
            cache_creation_cost_per_million_nanousd: 1_250_000_000,
            cache_read_cost_per_million_nanousd: 100_000_000,
        });
    }
    if normalized.contains("opus") {
        return Some(ModelPricing {
            input_cost_per_million_nanousd: 15_000_000_000,
            output_cost_per_million_nanousd: 75_000_000_000,
            cache_creation_cost_per_million_nanousd: 18_750_000_000,
            cache_read_cost_per_million_nanousd: 1_500_000_000,
        });
    }
    if normalized.contains("sonnet") {
        return Some(ModelPricing::default_sonnet_tier());
    }
    None
}

impl TokenUsage {
    #[must_use]
    pub fn total_tokens(self) -> u32 {
        self.input_tokens
            + self.output_tokens
            + self.cache_creation_input_tokens
            + self.cache_read_input_tokens
    }

    #[must_use]
    pub fn estimate_cost_usd(self) -> UsageCostEstimate {
        self.estimate_cost_usd_with_pricing(ModelPricing::default_sonnet_tier())
    }

    #[must_use]
    pub fn estimate_cost_usd_with_pricing(self, pricing: ModelPricing) -> UsageCostEstimate {
        UsageCostEstimate {
            input_cost_nanousd: cost_for_tokens(self.input_tokens, pricing.input_cost_per_million_nanousd),
            output_cost_nanousd: cost_for_tokens(self.output_tokens, pricing.output_cost_per_million_nanousd),
            cache_creation_cost_nanousd: cost_for_tokens(
                self.cache_creation_input_tokens,
                pricing.cache_creation_cost_per_million_nanousd,
            ),
            cache_read_cost_nanousd: cost_for_tokens(
                self.cache_read_input_tokens,
                pricing.cache_read_cost_per_million_nanousd,
            ),
        }
    }

    #[must_use]
    pub fn summary_lines(self, label: &str) -> Vec<String> {
        self.summary_lines_for_model(label, None)
    }

    #[must_use]
    pub fn summary_lines_for_model(self, label: &str, model: Option<&str>) -> Vec<String> {
        let pricing = model.and_then(pricing_for_model);
        let cost = pricing.map_or_else(
            || self.estimate_cost_usd(),
            |pricing| self.estimate_cost_usd_with_pricing(pricing),
        );
        let model_suffix =
            model.map_or_else(String::new, |model_name| format!(" model={model_name}"));
        let pricing_suffix = if pricing.is_some() {
            ""
        } else if model.is_some() {
            " pricing=estimated-default"
        } else {
            ""
        };
        vec![
            format!(
                "{label}: total_tokens={} input={} output={} cache_write={} cache_read={} estimated_cost={}{}{}",
                self.total_tokens(),
                self.input_tokens,
                self.output_tokens,
                self.cache_creation_input_tokens,
                self.cache_read_input_tokens,
                format_usd(cost.total_cost_nanousd()),
                model_suffix,
                pricing_suffix,
            ),
            format!(
                "  cost breakdown: input={} output={} cache_write={} cache_read={}",
                format_usd(cost.input_cost_nanousd),
                format_usd(cost.output_cost_nanousd),
                format_usd(cost.cache_creation_cost_nanousd),
                format_usd(cost.cache_read_cost_nanousd),
            ),
        ]
    }
}

/// Exact integer cost: tokens x nano-USD-per-million / 1_000_000, truncated to nano-USD.
/// u128 intermediate cannot overflow for any u32 token count.
fn cost_for_tokens(tokens: u32, nanousd_per_million_tokens: u64) -> u64 {
    ((tokens as u128 * nanousd_per_million_tokens as u128) / 1_000_000) as u64
}

#[must_use]
/// Formats a dollar-denominated value for CLI display.
pub fn format_usd(nanousd: u64) -> String {
    // four decimal places, integer division only — same output shape as before ($0.1234)
    let ten_thousandths = nanousd / 100_000; // 1e-4 USD units
    format!("${}.{:04}", ten_thousandths / 10_000, ten_thousandths % 10_000)
}

/// Aggregates token usage across a running session.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageTracker {
    latest_turn: TokenUsage,
    cumulative: TokenUsage,
    turns: u32,
}

impl UsageTracker {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_session(session: &Session) -> Self {
        let mut tracker = Self::new();
        for message in &session.messages {
            if let Some(usage) = message.usage {
                tracker.record(usage);
            }
        }
        tracker
    }

    pub fn record(&mut self, usage: TokenUsage) {
        self.latest_turn = usage;
        self.cumulative.input_tokens += usage.input_tokens;
        self.cumulative.output_tokens += usage.output_tokens;
        self.cumulative.cache_creation_input_tokens += usage.cache_creation_input_tokens;
        self.cumulative.cache_read_input_tokens += usage.cache_read_input_tokens;
        self.turns += 1;
    }

    #[must_use]
    pub fn current_turn_usage(&self) -> TokenUsage {
        self.latest_turn
    }

    #[must_use]
    pub fn cumulative_usage(&self) -> TokenUsage {
        self.cumulative
    }

    #[must_use]
    pub fn turns(&self) -> u32 {
        self.turns
    }
}

#[cfg(test)]
mod tests {
    use super::{format_usd, pricing_for_model, TokenUsage, UsageTracker};
    use crate::session::{ContentBlock, ConversationMessage, MessageRole, Session};

    #[test]
    fn tracks_true_cumulative_usage() {
        let mut tracker = UsageTracker::new();
        tracker.record(TokenUsage {
            input_tokens: 10,
            output_tokens: 4,
            cache_creation_input_tokens: 2,
            cache_read_input_tokens: 1,
        });
        tracker.record(TokenUsage {
            input_tokens: 20,
            output_tokens: 6,
            cache_creation_input_tokens: 3,
            cache_read_input_tokens: 2,
        });

        assert_eq!(tracker.turns(), 2);
        assert_eq!(tracker.current_turn_usage().input_tokens, 20);
        assert_eq!(tracker.current_turn_usage().output_tokens, 6);
        assert_eq!(tracker.cumulative_usage().output_tokens, 10);
        assert_eq!(tracker.cumulative_usage().input_tokens, 30);
        assert_eq!(tracker.cumulative_usage().total_tokens(), 48);
    }

    #[test]
    fn computes_cost_summary_lines() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            cache_creation_input_tokens: 100_000,
            cache_read_input_tokens: 200_000,
        };

        let cost = usage.estimate_cost_usd();
        assert_eq!(format_usd(cost.input_cost_nanousd), "$15.0000");
        assert_eq!(format_usd(cost.output_cost_nanousd), "$37.5000");
        let lines = usage.summary_lines_for_model("usage", Some("claude-sonnet-4-20250514"));
        assert!(lines[0].contains("estimated_cost=$54.6750"));
        assert!(lines[0].contains("model=claude-sonnet-4-20250514"));
        assert!(lines[1].contains("cache_read=$0.3000"));
    }

    #[test]
    fn supports_model_specific_pricing() {
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 500_000,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };

        let haiku = pricing_for_model("claude-haiku-4-5-20251001").expect("haiku pricing");
        let opus = pricing_for_model("claude-opus-4-6").expect("opus pricing");
        let haiku_cost = usage.estimate_cost_usd_with_pricing(haiku);
        let opus_cost = usage.estimate_cost_usd_with_pricing(opus);
        assert_eq!(format_usd(haiku_cost.total_cost_nanousd()), "$3.5000");
        assert_eq!(format_usd(opus_cost.total_cost_nanousd()), "$52.5000");
    }

    #[test]
    fn marks_unknown_model_pricing_as_fallback() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 100,
            cache_creation_input_tokens: 0,
            cache_read_input_tokens: 0,
        };
        let lines = usage.summary_lines_for_model("usage", Some("custom-model"));
        assert!(lines[0].contains("pricing=estimated-default"));
    }

    #[test]
    fn reconstructs_usage_from_session_messages() {
        let mut session = Session::new();
        session.messages = vec![ConversationMessage {
            role: MessageRole::Assistant,
            blocks: vec![ContentBlock::Text {
                text: "done".to_string(),
            }],
            usage: Some(TokenUsage {
                input_tokens: 5,
                output_tokens: 2,
                cache_creation_input_tokens: 1,
                cache_read_input_tokens: 0,
            }),
        }];

        let tracker = UsageTracker::from_session(&session);
        assert_eq!(tracker.turns(), 1);
        assert_eq!(tracker.cumulative_usage().total_tokens(), 8);
    }
}
