"""HuggingFace Inference API model endpoints (current as of March 2026)."""

LLM_ENDPOINTS = [
    # ── Text Generation ───────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api-inference.huggingface.co/models/THUDM/GLM-5",
        "params": {
            "name": "GLM-5 (HuggingFace)",
            "provider": "huggingface",
            "auth": {"method": "api_key", "key_name": "HF_TOKEN", "signup_url": "https://huggingface.co/settings/tokens"},
            "protocol": "huggingface",
            "pricing": {"model": "freemium", "free_tier": "rate-limited free tier"},
            "model": "THUDM/GLM-5",
            "description": "GLM-5 — top-ranked open-weight model, reasoning",
        },
    },
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api-inference.huggingface.co/models/deepseek-ai/DeepSeek-V3.2",
        "params": {
            "name": "DeepSeek V3.2 (HuggingFace)",
            "provider": "huggingface",
            "auth": {"method": "api_key", "key_name": "HF_TOKEN", "signup_url": "https://huggingface.co/settings/tokens"},
            "protocol": "huggingface",
            "pricing": {"model": "freemium", "free_tier": "rate-limited free tier"},
            "model": "deepseek-ai/DeepSeek-V3.2",
            "description": "DeepSeek V3.2 — latest MoE, MIT license, frontier reasoning",
        },
    },
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api-inference.huggingface.co/models/Qwen/Qwen3.5-397B-A17B",
        "params": {
            "name": "Qwen3.5 397B (HuggingFace)",
            "provider": "huggingface",
            "auth": {"method": "api_key", "key_name": "HF_TOKEN", "signup_url": "https://huggingface.co/settings/tokens"},
            "protocol": "huggingface",
            "pricing": {"model": "freemium", "free_tier": "rate-limited free tier"},
            "model": "Qwen/Qwen3.5-397B-A17B",
            "description": "Qwen3.5 — 397B total / 17B active MoE, multilingual, Apache 2.0",
        },
    },
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api-inference.huggingface.co/models/moonshotai/Kimi-K2.5",
        "params": {
            "name": "Kimi K2.5 (HuggingFace)",
            "provider": "huggingface",
            "auth": {"method": "api_key", "key_name": "HF_TOKEN", "signup_url": "https://huggingface.co/settings/tokens"},
            "protocol": "huggingface",
            "pricing": {"model": "freemium", "free_tier": "rate-limited free tier"},
            "model": "moonshotai/Kimi-K2.5",
            "description": "Kimi K2.5 — #2 open-weight model, reasoning, Moonshot AI",
        },
    },
    # ── Reasoning ─────────────────────────────────────────────────────────
    {
        "type": "compute/inference/reasoning",
        "endpoint": "https://api-inference.huggingface.co/models/deepseek-ai/DeepSeek-R1",
        "params": {
            "name": "DeepSeek R1 (HuggingFace)",
            "provider": "huggingface",
            "auth": {"method": "api_key", "key_name": "HF_TOKEN", "signup_url": "https://huggingface.co/settings/tokens"},
            "protocol": "huggingface",
            "pricing": {"model": "freemium", "free_tier": "rate-limited free tier"},
            "model": "deepseek-ai/DeepSeek-R1",
            "description": "DeepSeek R1 — chain-of-thought reasoning, competitive with o3",
        },
    },
    # ── Code Generation ───────────────────────────────────────────────────
    {
        "type": "compute/inference/code-generation",
        "endpoint": "https://api-inference.huggingface.co/models/Qwen/Qwen3-Coder-Next",
        "params": {
            "name": "Qwen3 Coder (HuggingFace)",
            "provider": "huggingface",
            "auth": {"method": "api_key", "key_name": "HF_TOKEN", "signup_url": "https://huggingface.co/settings/tokens"},
            "protocol": "huggingface",
            "pricing": {"model": "freemium", "free_tier": "rate-limited free tier"},
            "model": "Qwen/Qwen3-Coder-Next",
            "description": "Qwen3 Coder — optimized for agentic coding workflows",
        },
    },
    # ── Embeddings ────────────────────────────────────────────────────────
    {
        "type": "compute/inference/embeddings",
        "endpoint": "https://api-inference.huggingface.co/models/BAAI/bge-large-en-v1.5",
        "params": {
            "name": "BGE Large Embeddings (HuggingFace)",
            "provider": "huggingface",
            "auth": {"method": "api_key", "key_name": "HF_TOKEN", "signup_url": "https://huggingface.co/settings/tokens"},
            "protocol": "huggingface",
            "pricing": {"model": "freemium", "free_tier": "rate-limited free tier"},
            "model": "BAAI/bge-large-en-v1.5",
            "description": "State-of-the-art text embeddings for RAG and semantic search",
        },
    },
    # ── Image Generation ──────────────────────────────────────────────────
    {
        "type": "compute/inference/image-generation",
        "endpoint": "https://api-inference.huggingface.co/models/black-forest-labs/FLUX.1-dev",
        "params": {
            "name": "FLUX.1 Dev (HuggingFace)",
            "provider": "huggingface",
            "auth": {"method": "api_key", "key_name": "HF_TOKEN", "signup_url": "https://huggingface.co/settings/tokens"},
            "protocol": "huggingface",
            "pricing": {"model": "freemium", "free_tier": "rate-limited free tier"},
            "model": "black-forest-labs/FLUX.1-dev",
            "description": "FLUX.1 — state-of-the-art image generation from Black Forest Labs",
        },
    },
    # ── Speech ────────────────────────────────────────────────────────────
    {
        "type": "compute/inference/speech-to-text",
        "endpoint": "https://api-inference.huggingface.co/models/openai/whisper-large-v3-turbo",
        "params": {
            "name": "Whisper Large v3 Turbo (HuggingFace)",
            "provider": "huggingface",
            "auth": {"method": "api_key", "key_name": "HF_TOKEN", "signup_url": "https://huggingface.co/settings/tokens"},
            "protocol": "huggingface",
            "pricing": {"model": "freemium", "free_tier": "rate-limited free tier"},
            "model": "openai/whisper-large-v3-turbo",
            "description": "Whisper v3 Turbo — fast, accurate speech recognition",
        },
    },
    # ── Translation ───────────────────────────────────────────────────────
    {
        "type": "compute/inference/translation",
        "endpoint": "https://api-inference.huggingface.co/models/facebook/nllb-200-distilled-600M",
        "params": {
            "name": "NLLB Translation (HuggingFace)",
            "provider": "huggingface",
            "auth": {"method": "api_key", "key_name": "HF_TOKEN", "signup_url": "https://huggingface.co/settings/tokens"},
            "protocol": "huggingface",
            "pricing": {"model": "freemium", "free_tier": "rate-limited free tier"},
            "model": "facebook/nllb-200-distilled-600M",
            "description": "No Language Left Behind — 200+ language translation",
        },
    },
]
