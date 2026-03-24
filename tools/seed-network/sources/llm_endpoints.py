"""Public LLM inference endpoints for mesh seeding."""

LLM_ENDPOINTS = [
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api-inference.huggingface.co/models/meta-llama/Llama-3.3-70B-Instruct",
        "params": {
            "name": "Llama 3.3 70B (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "meta-llama/Llama-3.3-70B-Instruct",
        },
    },
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api-inference.huggingface.co/models/mistralai/Mistral-7B-Instruct-v0.3",
        "params": {
            "name": "Mistral 7B (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "mistralai/Mistral-7B-Instruct-v0.3",
        },
    },
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api-inference.huggingface.co/models/google/gemma-2-9b-it",
        "params": {
            "name": "Gemma 2 9B (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "google/gemma-2-9b-it",
        },
    },
    {
        "type": "compute/inference/embeddings",
        "endpoint": "https://api-inference.huggingface.co/models/BAAI/bge-large-en-v1.5",
        "params": {
            "name": "BGE Large Embeddings (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "BAAI/bge-large-en-v1.5",
        },
    },
    {
        "type": "compute/inference/image-generation",
        "endpoint": "https://api-inference.huggingface.co/models/stabilityai/stable-diffusion-xl-base-1.0",
        "params": {
            "name": "SDXL (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "stabilityai/stable-diffusion-xl-base-1.0",
        },
    },
    {
        "type": "compute/inference/speech-to-text",
        "endpoint": "https://api-inference.huggingface.co/models/openai/whisper-large-v3",
        "params": {
            "name": "Whisper Large v3 (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "openai/whisper-large-v3",
        },
    },
    {
        "type": "compute/inference/text-to-speech",
        "endpoint": "https://api-inference.huggingface.co/models/facebook/mms-tts-eng",
        "params": {
            "name": "MMS TTS English (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
        },
    },
    {
        "type": "compute/inference/translation",
        "endpoint": "https://api-inference.huggingface.co/models/facebook/nllb-200-distilled-600M",
        "params": {
            "name": "NLLB Translation (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "facebook/nllb-200-distilled-600M",
        },
    },
    {
        "type": "compute/inference/code-generation",
        "endpoint": "https://api-inference.huggingface.co/models/bigcode/starcoder2-15b",
        "params": {
            "name": "StarCoder2 15B (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "bigcode/starcoder2-15b",
        },
    },
    {
        "type": "compute/inference/object-detection",
        "endpoint": "https://api-inference.huggingface.co/models/facebook/detr-resnet-50",
        "params": {
            "name": "DETR Object Detection (HuggingFace)",
            "provider": "huggingface",
            "auth": "HF_TOKEN",
            "model": "facebook/detr-resnet-50",
        },
    },
]
