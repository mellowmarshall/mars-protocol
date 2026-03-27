"""Inference API providers — commercial and open platforms with API access."""

INFERENCE_PROVIDERS = [
    # ── Together AI ───────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.together.xyz/v1/chat/completions",
        "params": {
            "name": "Together AI",
            "description": "Fast inference for 100+ open models — Llama, Mistral, Qwen, and more",
            "auth": {"method": "api_key", "key_name": "TOGETHER_API_KEY", "signup_url": "https://together.ai"},
            "protocol": "openai",
            "pricing": {"model": "freemium", "free_tier": "$5 free credits", "paid_url": "https://together.ai/pricing"},
            "docs": "https://docs.together.ai",
            "models": ["THUDM/GLM-5", "deepseek-ai/DeepSeek-V3.2", "Qwen/Qwen3.5-397B", "moonshotai/Kimi-K2.5"],
        },
    },
    # ── Groq ──────────────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.groq.com/openai/v1/chat/completions",
        "params": {
            "name": "Groq",
            "description": "Fastest inference — LPU hardware, sub-100ms latency",
            "auth": {"method": "api_key", "key_name": "GROQ_API_KEY", "signup_url": "https://console.groq.com"},
            "protocol": "openai",
            "pricing": {"model": "per-token", "price_per_mtok": 0.59, "currency": "USD"},
            "docs": "https://console.groq.com/docs",
            "models": ["glm-5", "deepseek-v3.2", "kimi-k2.5"],
        },
    },
    # ── Fireworks ─────────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.fireworks.ai/inference/v1/chat/completions",
        "params": {
            "name": "Fireworks AI",
            "description": "Serverless inference with function calling and JSON mode",
            "auth": {"method": "api_key", "key_name": "FIREWORKS_API_KEY", "signup_url": "https://fireworks.ai"},
            "protocol": "openai",
            "pricing": {"model": "freemium", "free_tier": "$1 free credits", "paid_url": "https://fireworks.ai/pricing"},
            "docs": "https://docs.fireworks.ai",
            "models": ["accounts/fireworks/models/glm-5", "accounts/fireworks/models/deepseek-v3"],
        },
    },
    # ── DeepInfra ─────────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.deepinfra.com/v1/openai/chat/completions",
        "params": {
            "name": "DeepInfra",
            "description": "Low-cost inference — optimized for throughput",
            "auth": {"method": "api_key", "key_name": "DEEPINFRA_API_KEY", "signup_url": "https://deepinfra.com"},
            "protocol": "openai",
            "pricing": {"model": "per-token", "price_per_mtok": 0.65, "currency": "USD"},
            "docs": "https://deepinfra.com/docs",
        },
    },
    # ── Cerebras ──────────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.cerebras.ai/v1/chat/completions",
        "params": {
            "name": "Cerebras",
            "description": "Wafer-scale engine — 2000+ tokens/sec, fastest output speed",
            "auth": {"method": "api_key", "key_name": "CEREBRAS_API_KEY", "signup_url": "https://inference-docs.cerebras.ai"},
            "protocol": "openai",
            "pricing": {"model": "per-token", "price_per_mtok": 0.60, "currency": "USD"},
            "docs": "https://inference-docs.cerebras.ai",
        },
    },
    # ── Vast.ai Serverless ────────────────────────────────────────────────
    {
        "type": "compute/inference/serverless",
        "endpoint": "https://api.vast.ai/v1",
        "params": {
            "name": "Vast.ai Serverless",
            "description": "GPU marketplace with serverless inference — A100s to consumer GPUs",
            "auth": {"method": "api_key", "key_name": "VAST_API_KEY", "signup_url": "https://vast.ai"},
            "protocol": "vast",
            "pricing": {"model": "per-hour", "price_per_hour": 0.40, "currency": "USD"},
            "docs": "https://docs.vast.ai/serverless",
            "gpu_types": ["RTX 4090", "A100", "H100", "RTX 3090"],
        },
    },
    # ── RunPod Serverless ─────────────────────────────────────────────────
    {
        "type": "compute/inference/serverless",
        "endpoint": "https://api.runpod.ai/v2",
        "params": {
            "name": "RunPod Serverless",
            "description": "Serverless GPU endpoints — autoscaling, pay-per-second",
            "auth": {"method": "api_key", "key_name": "RUNPOD_API_KEY", "signup_url": "https://runpod.io"},
            "protocol": "runpod",
            "pricing": {"model": "per-hour", "price_per_hour": 0.59, "currency": "USD"},
            "docs": "https://docs.runpod.io/serverless",
            "gpu_types": ["A100", "A10G", "RTX 4090"],
        },
    },
    # ── Lepton AI ─────────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://llama3-3-70b.lepton.run/api/v1/chat/completions",
        "params": {
            "name": "Lepton AI",
            "description": "Optimized open-model inference with OpenAI-compatible API",
            "auth": {"method": "api_key", "key_name": "LEPTON_API_KEY", "signup_url": "https://www.lepton.ai"},
            "protocol": "openai",
            "pricing": {"model": "freemium", "free_tier": "$10 free credits", "paid_url": "https://www.lepton.ai/pricing"},
            "docs": "https://www.lepton.ai/docs",
        },
    },
    # ── Anyscale / Endpoints ──────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.endpoints.anyscale.com/v1/chat/completions",
        "params": {
            "name": "Anyscale Endpoints",
            "description": "Fine-tuning + serving on Ray — production-grade open model hosting",
            "auth": {"method": "api_key", "key_name": "ANYSCALE_API_KEY", "signup_url": "https://anyscale.com"},
            "protocol": "openai",
            "pricing": {"model": "per-token", "price_per_mtok": 0.50, "currency": "USD"},
            "docs": "https://docs.anyscale.com",
        },
    },
    # ── Replicate ─────────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.replicate.com/v1/predictions",
        "params": {
            "name": "Replicate",
            "description": "Run any ML model via API — LLMs, image gen, audio, video",
            "auth": {"method": "api_key", "key_name": "REPLICATE_API_TOKEN", "signup_url": "https://replicate.com"},
            "protocol": "replicate",
            "pricing": {"model": "per-token", "price_per_mtok": 0.65, "currency": "USD"},
            "docs": "https://replicate.com/docs",
        },
    },
    {
        "type": "compute/inference/image-generation",
        "endpoint": "https://api.replicate.com/v1/predictions",
        "params": {
            "name": "Replicate (Image)",
            "description": "FLUX, SDXL, Stable Diffusion via API",
            "auth": {"method": "api_key", "key_name": "REPLICATE_API_TOKEN", "signup_url": "https://replicate.com"},
            "protocol": "replicate",
            "pricing": {"model": "per-token", "price_per_mtok": 0.65, "currency": "USD"},
            "docs": "https://replicate.com/docs",
        },
    },
    # ── Perplexity ────────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.perplexity.ai/chat/completions",
        "params": {
            "name": "Perplexity API",
            "description": "Search-augmented LLM — answers with citations from the web",
            "auth": {"method": "api_key", "key_name": "PERPLEXITY_API_KEY", "signup_url": "https://perplexity.ai"},
            "protocol": "openai",
            "pricing": {"model": "per-token", "price_per_mtok": 1.00, "currency": "USD"},
            "docs": "https://docs.perplexity.ai",
        },
    },
    # ── Mistral AI ────────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.mistral.ai/v1/chat/completions",
        "params": {
            "name": "Mistral AI",
            "description": "Official Mistral API — Mistral Large, Medium, Small, Codestral",
            "auth": {"method": "api_key", "key_name": "MISTRAL_API_KEY", "signup_url": "https://console.mistral.ai"},
            "protocol": "openai",
            "pricing": {"model": "freemium", "free_tier": "free tier for Mistral Small", "paid_url": "https://mistral.ai/pricing"},
            "docs": "https://docs.mistral.ai",
        },
    },
]
