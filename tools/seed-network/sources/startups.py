"""Startup APIs with free tiers — companies that want agent distribution."""

STARTUP_APIS = [
    # ── AI-Native Search ──────────────────────────────────────────────────
    {
        "type": "data/search/ai",
        "endpoint": "https://api.tavily.com/search",
        "params": {
            "name": "Tavily",
            "description": "AI-optimized web search — aggregates 20 sources, scores and ranks for relevance",
            "auth": {"method": "api_key", "key_name": "TAVILY_API_KEY", "signup_url": "https://tavily.com"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "1,000 credits/month", "paid_url": "https://tavily.com/pricing"},
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
            "auth": {"method": "api_key", "key_name": "EXA_API_KEY", "signup_url": "https://exa.ai"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "1,000 credits", "paid_url": "https://exa.ai/pricing"},
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
            "auth": {"method": "api_key", "key_name": "SERPER_API_KEY", "signup_url": "https://serper.dev"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "2,500 queries free, no card required", "paid_url": "https://serper.dev/pricing"},
            "docs": "https://serper.dev/docs",
        },
    },
    {
        "type": "data/search/web",
        "endpoint": "https://api.search.brave.com/res/v1/web/search",
        "params": {
            "name": "Brave Search API",
            "description": "Independent web search with AI snippets",
            "auth": {"method": "api_key", "key_name": "BRAVE_API_KEY", "signup_url": "https://brave.com/search/api/"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "2,000 queries/month", "paid_url": "https://brave.com/search/api/"},
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
            "auth": {"method": "api_key", "key_name": "FIRECRAWL_API_KEY", "signup_url": "https://firecrawl.dev"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "500 credits/month, no card required", "paid_url": "https://firecrawl.dev/pricing"},
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
            "auth": {"method": "api_key", "key_name": "APIFY_TOKEN", "signup_url": "https://apify.com"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "$5/month free", "paid_url": "https://apify.com/pricing"},
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
            "auth": {"method": "none"},
            "protocol": "rest",
            "pricing": {"model": "free"},
            "docs": "https://developer.puter.com",
        },
    },
    {
        "type": "compute/inference/text-generation",
        "endpoint": "https://openrouter.ai/api/v1/chat/completions",
        "params": {
            "name": "OpenRouter",
            "description": "API aggregator — hundreds of free and paid LLMs through one endpoint",
            "auth": {"method": "api_key", "key_name": "OPENROUTER_API_KEY", "signup_url": "https://openrouter.ai"},
            "protocol": "openai",
            "pricing": {"model": "freemium", "free_tier": "$5 free credits for new users", "paid_url": "https://openrouter.ai/pricing"},
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
            "auth": {"method": "api_key", "key_name": "RESEND_API_KEY", "signup_url": "https://resend.com"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "3,000 emails/month, 100/day", "paid_url": "https://resend.com/pricing"},
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
            "auth": {"method": "api_key", "key_name": "NEON_API_KEY", "signup_url": "https://neon.tech"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "0.5 GB storage, always free", "paid_url": "https://neon.tech/pricing"},
            "docs": "https://neon.tech/docs",
        },
    },
    {
        "type": "compute/database/redis",
        "endpoint": "https://api.upstash.com",
        "params": {
            "name": "Upstash Redis",
            "description": "Serverless Redis — agent memory, caching, rate limiting",
            "auth": {"method": "api_key", "key_name": "UPSTASH_TOKEN", "signup_url": "https://upstash.com"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "10,000 commands/day", "paid_url": "https://upstash.com/pricing"},
            "docs": "https://upstash.com/docs/redis",
        },
    },
    {
        "type": "compute/database/vector",
        "endpoint": "https://api.upstash.com/v2/vector",
        "params": {
            "name": "Upstash Vector",
            "description": "Serverless vector database for RAG and semantic search",
            "auth": {"method": "api_key", "key_name": "UPSTASH_TOKEN", "signup_url": "https://upstash.com"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "10,000 vectors, 1,000 queries/day", "paid_url": "https://upstash.com/pricing"},
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
            "auth": {"method": "api_key", "key_name": "UNSTRUCTURED_API_KEY", "signup_url": "https://unstructured.io"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "1,000 pages/month", "paid_url": "https://unstructured.io/pricing"},
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
            "auth": {"method": "api_key", "key_name": "TOGETHER_API_KEY", "signup_url": "https://together.ai"},
            "protocol": "openai",
            "pricing": {"model": "freemium", "free_tier": "$5 free credits", "paid_url": "https://together.ai/pricing"},
            "docs": "https://docs.together.ai",
        },
    },
    {
        "type": "compute/inference/image-generation",
        "endpoint": "https://fal.run",
        "params": {
            "name": "fal.ai",
            "description": "Fast image generation — FLUX, SDXL, ControlNet",
            "auth": {"method": "api_key", "key_name": "FAL_KEY", "signup_url": "https://fal.ai"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "$10 free credits", "paid_url": "https://fal.ai/pricing"},
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
            "auth": {"method": "api_key", "key_name": "E2B_API_KEY", "signup_url": "https://e2b.dev"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "100 sandbox hours/month", "paid_url": "https://e2b.dev/pricing"},
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
            "auth": {"method": "api_key", "key_name": "LANGFUSE_API_KEY", "signup_url": "https://langfuse.com"},
            "protocol": "rest",
            "pricing": {"model": "freemium", "free_tier": "50K observations/month", "paid_url": "https://langfuse.com/pricing"},
            "docs": "https://langfuse.com/docs",
            "github": "https://github.com/langfuse/langfuse",
        },
    },
]
