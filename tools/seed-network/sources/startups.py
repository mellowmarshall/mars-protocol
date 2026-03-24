"""Startup APIs with free tiers — companies that want agent distribution."""

STARTUP_APIS = [
    # ── AI-Native Search ──────────────────────────────────────────────────
    {
        "type": "data/search/ai",
        "endpoint": "https://api.tavily.com/search",
        "params": {
            "name": "Tavily",
            "description": "AI-optimized web search — aggregates 20 sources, scores and ranks for relevance",
            "free_tier": "1,000 credits/month",
            "auth": "TAVILY_API_KEY",
            "docs": "https://docs.tavily.com",
            "github": "https://github.com/tavily-ai/tavily-python",
        },
    },
    {
        "type": "data/search/ai",
        "endpoint": "https://api.exa.ai/search",
        "params": {
            "name": "Exa",
            "description": "Neural search engine with embeddings-powered next-link prediction",
            "free_tier": "1,000 credits",
            "auth": "EXA_API_KEY",
            "docs": "https://docs.exa.ai",
            "github": "https://github.com/exa-labs/exa-py",
        },
    },
    {
        "type": "data/search/serp",
        "endpoint": "https://google.serper.dev/search",
        "params": {
            "name": "Serper",
            "description": "Google SERP API — search, images, news, maps, shopping, scholar",
            "free_tier": "2,500 queries free, no card required",
            "auth": "SERPER_API_KEY",
            "docs": "https://serper.dev/docs",
        },
    },
    {
        "type": "data/search/web",
        "endpoint": "https://api.search.brave.com/res/v1/web/search",
        "params": {
            "name": "Brave Search API",
            "description": "Independent web search with AI snippets",
            "free_tier": "2,000 queries/month",
            "auth": "BRAVE_API_KEY",
            "docs": "https://brave.com/search/api/",
        },
    },
    # ── Web Scraping / Crawling ───────────────────────────────────────────
    {
        "type": "data/scraping/web",
        "endpoint": "https://api.firecrawl.dev/v1/scrape",
        "params": {
            "name": "Firecrawl",
            "description": "Web scraping API — turns websites into LLM-ready data",
            "free_tier": "500 credits/month, no card required",
            "auth": "FIRECRAWL_API_KEY",
            "docs": "https://docs.firecrawl.dev",
            "github": "https://github.com/mendableai/firecrawl",
        },
    },
    {
        "type": "data/scraping/web",
        "endpoint": "https://api.apify.com/v2",
        "params": {
            "name": "Apify",
            "description": "Web scraping platform with 1,600+ ready-made scrapers",
            "free_tier": "$5/month free",
            "auth": "APIFY_TOKEN",
            "docs": "https://docs.apify.com/api/v2",
        },
    },
    # ── Free LLM Access ───────────────────────────────────────────────────
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://api.puter.com/ai/chat",
        "params": {
            "name": "Puter.js (Free GPT)",
            "description": "Free GPT access — no API key required, runs from frontend",
            "free_tier": "unlimited",
            "auth": "none",
            "docs": "https://developer.puter.com",
        },
    },
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://openrouter.ai/api/v1/chat/completions",
        "params": {
            "name": "OpenRouter",
            "description": "API aggregator — hundreds of free and paid LLMs through one endpoint",
            "free_tier": "$5 free credits for new users",
            "auth": "OPENROUTER_API_KEY",
            "docs": "https://openrouter.ai/docs",
        },
    },
    # ── Email ─────────────────────────────────────────────────────────────
    {
        "type": "compute/messaging/email",
        "endpoint": "https://api.resend.com/emails",
        "params": {
            "name": "Resend",
            "description": "Developer-first email API",
            "free_tier": "3,000 emails/month, 100/day",
            "auth": "RESEND_API_KEY",
            "docs": "https://resend.com/docs",
            "github": "https://github.com/resend/resend-python",
        },
    },
    # ── Database / Storage ────────────────────────────────────────────────
    {
        "type": "compute/database/postgres",
        "endpoint": "https://console.neon.tech",
        "params": {
            "name": "Neon Serverless Postgres",
            "description": "Serverless Postgres with branching — agents can spin up databases",
            "free_tier": "0.5 GB storage, always free",
            "auth": "NEON_API_KEY",
            "docs": "https://neon.tech/docs",
        },
    },
    {
        "type": "compute/database/redis",
        "endpoint": "https://api.upstash.com",
        "params": {
            "name": "Upstash Redis",
            "description": "Serverless Redis — agent memory, caching, rate limiting",
            "free_tier": "10,000 commands/day",
            "auth": "UPSTASH_TOKEN",
            "docs": "https://upstash.com/docs/redis",
        },
    },
    {
        "type": "compute/database/vector",
        "endpoint": "https://api.upstash.com/v2/vector",
        "params": {
            "name": "Upstash Vector",
            "description": "Serverless vector database for RAG and semantic search",
            "free_tier": "10,000 vectors, 1,000 queries/day",
            "auth": "UPSTASH_TOKEN",
            "docs": "https://upstash.com/docs/vector",
        },
    },
    # ── Document Processing ───────────────────────────────────────────────
    {
        "type": "compute/document/parsing",
        "endpoint": "https://api.unstructured.io/general/v0/general",
        "params": {
            "name": "Unstructured.io",
            "description": "Parse PDFs, Word docs, HTML into structured data for LLMs",
            "free_tier": "1,000 pages/month",
            "auth": "UNSTRUCTURED_API_KEY",
            "docs": "https://docs.unstructured.io",
        },
    },
    # ── Image Generation ──────────────────────────────────────────────────
    {
        "type": "compute/inference/image-generation",
        "endpoint": "https://api.together.xyz/v1/images/generations",
        "params": {
            "name": "Together AI (FLUX)",
            "description": "FLUX and Stable Diffusion image generation",
            "free_tier": "$5 free credits",
            "auth": "TOGETHER_API_KEY",
            "docs": "https://docs.together.ai",
        },
    },
    {
        "type": "compute/inference/image-generation",
        "endpoint": "https://fal.run",
        "params": {
            "name": "fal.ai",
            "description": "Fast image generation — FLUX, SDXL, ControlNet",
            "free_tier": "$10 free credits",
            "auth": "FAL_KEY",
            "docs": "https://fal.ai/docs",
        },
    },
    # ── Code Execution ────────────────────────────────────────────────────
    {
        "type": "compute/sandbox/code",
        "endpoint": "https://api.e2b.dev",
        "params": {
            "name": "E2B Code Sandbox",
            "description": "Cloud sandboxes for AI agents to execute code safely",
            "free_tier": "100 sandbox hours/month",
            "auth": "E2B_API_KEY",
            "docs": "https://e2b.dev/docs",
            "github": "https://github.com/e2b-dev/e2b",
        },
    },
    # ── Monitoring / Observability ────────────────────────────────────────
    {
        "type": "compute/observability/tracing",
        "endpoint": "https://api.langfuse.com",
        "params": {
            "name": "Langfuse",
            "description": "Open-source LLM observability — traces, evals, prompt management",
            "free_tier": "50K observations/month",
            "auth": "LANGFUSE_API_KEY",
            "docs": "https://langfuse.com/docs",
            "github": "https://github.com/langfuse/langfuse",
        },
    },
]
