"""Inference API providers — commercial and open platforms with API access."""

INFERENCE_PROVIDERS = [
    # ── Together AI ───────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.together.xyz/v1/chat/completions",
        "params": {
            "name": "Together AI",
            "description": "Fast inference for 100+ open models — Llama, Mistral, Qwen, and more",
            "pricing": "$0.10-2.50/M tokens depending on model",
            "free_tier": "$5 free credits",
            "auth": "TOGETHER_API_KEY",
            "openai_compatible": True,
            "docs": "https://docs.together.ai",
            "models": ["meta-llama/Llama-4-Scout", "deepseek-ai/DeepSeek-V3", "Qwen/Qwen3-235B", "mistralai/Mistral-Large-3"],
        },
    },
    # ── Groq ──────────────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.groq.com/openai/v1/chat/completions",
        "params": {
            "name": "Groq",
            "description": "Fastest inference — LPU hardware, sub-100ms latency",
            "pricing": "$0.05-0.80/M tokens (Llama 70B: $0.59/M)",
            "free_tier": "free tier available",
            "auth": "GROQ_API_KEY",
            "openai_compatible": True,
            "docs": "https://console.groq.com/docs",
            "models": ["llama-4-scout-17b", "deepseek-r1-distill-llama-70b", "qwen-qwq-32b"],
        },
    },
    # ── Fireworks ─────────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.fireworks.ai/inference/v1/chat/completions",
        "params": {
            "name": "Fireworks AI",
            "description": "Serverless inference with function calling and JSON mode",
            "pricing": "$0.20-0.90/M tokens",
            "free_tier": "$1 free credits",
            "auth": "FIREWORKS_API_KEY",
            "openai_compatible": True,
            "docs": "https://docs.fireworks.ai",
            "models": ["accounts/fireworks/models/llama4-scout-instruct", "accounts/fireworks/models/deepseek-v3"],
        },
    },
    # ── DeepInfra ─────────────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.deepinfra.com/v1/openai/chat/completions",
        "params": {
            "name": "DeepInfra",
            "description": "Low-cost inference — optimized for throughput",
            "pricing": "$0.13-0.65/M tokens",
            "free_tier": "free tier available",
            "auth": "DEEPINFRA_API_KEY",
            "openai_compatible": True,
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
            "pricing": "$0.10-0.60/M tokens",
            "free_tier": "free tier available",
            "auth": "CEREBRAS_API_KEY",
            "openai_compatible": True,
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
            "pricing": "pay-per-second, varies by GPU ($0.40-2.00/hr)",
            "auth": "VAST_API_KEY",
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
            "pricing": "pay-per-second ($0.59-1.20/hr depending on GPU)",
            "auth": "RUNPOD_API_KEY",
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
            "pricing": "$0.40-0.80/M tokens",
            "free_tier": "$10 free credits",
            "auth": "LEPTON_API_KEY",
            "openai_compatible": True,
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
            "pricing": "$0.15-0.50/M tokens",
            "auth": "ANYSCALE_API_KEY",
            "openai_compatible": True,
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
            "pricing": "varies by model and hardware",
            "auth": "REPLICATE_API_TOKEN",
            "docs": "https://replicate.com/docs",
        },
    },
    {
        "type": "compute/inference/image-generation",
        "endpoint": "https://api.replicate.com/v1/predictions",
        "params": {
            "name": "Replicate (Image)",
            "description": "FLUX, SDXL, Stable Diffusion via API",
            "pricing": "~$0.003-0.05 per image",
            "auth": "REPLICATE_API_TOKEN",
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
            "pricing": "$0.20-1.00/M tokens",
            "auth": "PERPLEXITY_API_KEY",
            "openai_compatible": True,
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
            "pricing": "$0.15-2.00/M tokens",
            "free_tier": "free tier for Mistral Small",
            "auth": "MISTRAL_API_KEY",
            "openai_compatible": True,
            "docs": "https://docs.mistral.ai",
        },
    },
]
