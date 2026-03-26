"""HuggingFace Inference API model endpoints (current as of March 2026)."""

LLM_ENDPOINTS = [
    # ── Text Generation ───────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api-inference.huggingface.co/models/meta-llama/Llama-4-Scout-17B-16E-Instruct",
        "params": {
            "name": "Llama 4 Scout (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "meta-llama/Llama-4-Scout-17B-16E-Instruct",
            "description": "Meta Llama 4 Scout — MoE, 109B total / 17B active, 10M context",
        },
    },
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api-inference.huggingface.co/models/deepseek-ai/DeepSeek-V3-0324",
        "params": {
            "name": "DeepSeek V3 (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "deepseek-ai/DeepSeek-V3-0324",
            "description": "DeepSeek V3 — 671B MoE, MIT license, frontier reasoning",
        },
    },
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api-inference.huggingface.co/models/Qwen/Qwen3-235B-A22B",
        "params": {
            "name": "Qwen3 235B (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "Qwen/Qwen3-235B-A22B",
            "description": "Qwen3 — 235B total / 22B active MoE, multilingual, Apache 2.0",
        },
    },
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api-inference.huggingface.co/models/mistralai/Mistral-Large-3",
        "params": {
            "name": "Mistral Large 3 (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "mistralai/Mistral-Large-3",
            "description": "Mistral Large 3 — 675B total / 41B active MoE, Apache 2.0",
        },
    },
    # ── Reasoning ─────────────────────────────────────────────────────────
    {
        "type": "compute/inference/reasoning",
        "endpoint": "https://api-inference.huggingface.co/models/deepseek-ai/DeepSeek-R1",
        "params": {
            "name": "DeepSeek R1 (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
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
            "auth": "HF_TOKEN",
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
            "auth": "HF_TOKEN",
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
            "auth": "HF_TOKEN",
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
            "auth": "HF_TOKEN",
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
            "auth": "HF_TOKEN",
            "model": "facebook/nllb-200-distilled-600M",
            "description": "No Language Left Behind — 200+ language translation",
        },
    },
]
