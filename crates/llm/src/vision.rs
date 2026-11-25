//! Which models can see images, for providers that cannot be asked.
//!
//! OpenAI-compatible servers (OpenAI, OpenRouter, Groq, LM Studio) have no common way to ask a
//! model what it accepts, and there is no setting for it, so this is a conservative name check
//! over model families known to take images. A model that is not listed is treated as text-only:
//! sending it an image would fail the request, while missing one only means Cilica reads the
//! text found in images instead. Ollama is asked directly (see [`crate::ollama::sees_images`]).

/// Whether a model id names a family known to accept images. Provider prefixes
/// (`openai/gpt-4o`, `google/gemini-2.5-pro`) and case are ignored.
pub fn name_sees_images(model: &str) -> bool {
    let lower = model.trim().to_lowercase();
    let name = lower.rsplit('/').next().unwrap_or(&lower);
    let has = |s: &str| name.contains(s);
    let starts = |s: &str| name.starts_with(s);

    // Families that say so in their name
    if has("vision") || has("-vl") || has("vl-") || has("llava") || has("pixtral") || has("minicpm-v") || has("moondream") {
        return true;
    }

    // OpenAI. The small o-series reasoning models (o1-mini, o3-mini) are text-only.
    if has("gpt-4o") || has("gpt-4.1") || has("gpt-4.5") || has("gpt-4-turbo") || starts("gpt-5") || starts("chatgpt-4o") {
        return true;
    }
    if (starts("o1") || starts("o3")) && !has("mini") && !has("preview") {
        return true;
    }
    if starts("o4-mini") {
        return true;
    }

    // Anthropic and Google models through a gateway: every current one takes images.
    if starts("claude-") || starts("gemini") {
        return true;
    }

    // Open-weight families with vision in every size Cilica could run
    if has("llama-4") || has("llama4") || has("qwen3-vl") || has("qwen3.8") || has("ministral-3") || has("gemma4") || has("gemma-4") {
        return true;
    }
    if has("mistral-small-3.1") || has("mistral-small3.1") || has("mistral-small-3.2") || has("mistral-small3.2") || has("mistral-medium") {
        return true;
    }
    // Gemma 3 sees images from 4B up; the 1B and 270M models are text-only.
    if (has("gemma-3") || has("gemma3")) && !has("1b") && !has("270m") {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vision_families_are_recognised() {
        for m in [
            "gpt-4o",
            "gpt-4o-mini",
            "gpt-4.1-nano",
            "gpt-5",
            "gpt-5-mini",
            "o3",
            "o4-mini",
            "openai/gpt-4o",
            "anthropic/claude-sonnet-4.5",
            "google/gemini-2.5-flash",
            "meta-llama/llama-4-scout-17b-16e-instruct",
            "qwen/qwen2.5-vl-72b-instruct",
            "qwen3-vl-8b",
            "mistralai/pixtral-12b",
            "mistral-small-3.2-24b-instruct",
            "google/gemma-3-27b-it",
            "llama-3.2-11b-vision-preview",
        ] {
            assert!(name_sees_images(m), "{m}");
        }
    }

    #[test]
    fn text_only_models_are_not() {
        for m in ["o3-mini", "o1-mini", "gpt-3.5-turbo", "llama-3.3-70b-versatile", "qwen3:8b", "deepseek-chat", "gemma-3-1b-it", "gpt-oss-20b", ""] {
            assert!(!name_sees_images(m), "{m}");
        }
    }
}
