"""Curated list of real, working public APIs for mesh seeding."""

PUBLIC_APIS = [
    # Search
    {
        "type": "data/search/web",
        "endpoint": "https://api.duckduckgo.com/",
        "params": {
            "name": "DuckDuckGo Instant Answer",
            "format": "json",
            "auth": "none",
            "docs": "https://api.duckduckgo.com/api",
        },
    },
    {
        "type": "data/search/web",
        "endpoint": "https://en.wikipedia.org/w/api.php",
        "params": {
            "name": "Wikipedia",
            "format": "json",
            "auth": "none",
            "docs": "https://www.mediawiki.org/wiki/API:Main_page",
        },
    },
    # Weather
    {
        "type": "data/weather/current",
        "endpoint": "https://api.open-meteo.com/v1/forecast",
        "params": {
            "name": "Open-Meteo",
            "auth": "none",
            "docs": "https://open-meteo.com/en/docs",
        },
    },
    {
        "type": "data/weather/forecast",
        "endpoint": "https://api.open-meteo.com/v1/forecast",
        "params": {"name": "Open-Meteo Forecast", "auth": "none"},
    },
    # Geocoding
    {
        "type": "data/geo/geocoding",
        "endpoint": "https://nominatim.openstreetmap.org/search",
        "params": {
            "name": "Nominatim (OpenStreetMap)",
            "format": "json",
            "auth": "none",
            "docs": "https://nominatim.org/release-docs/latest/api/Search/",
        },
    },
    {
        "type": "data/geo/reverse",
        "endpoint": "https://nominatim.openstreetmap.org/reverse",
        "params": {"name": "Nominatim Reverse", "auth": "none"},
    },
    # Time/Date
    {
        "type": "data/time/timezone",
        "endpoint": "http://worldtimeapi.org/api/timezone",
        "params": {"name": "WorldTimeAPI", "format": "json", "auth": "none"},
    },
    # Exchange rates
    {
        "type": "data/finance/exchange-rates",
        "endpoint": "https://api.exchangerate-api.com/v4/latest/USD",
        "params": {
            "name": "ExchangeRate-API",
            "auth": "none",
            "docs": "https://www.exchangerate-api.com/docs/overview",
        },
    },
    # IP/Network
    {
        "type": "data/network/ip-info",
        "endpoint": "https://ipapi.co/json/",
        "params": {"name": "ipapi.co", "auth": "none"},
    },
    {
        "type": "data/network/ip-geolocation",
        "endpoint": "https://ipinfo.io/json",
        "params": {"name": "IPinfo", "auth": "free-tier"},
    },
    # Random/Utility
    {
        "type": "data/utility/uuid",
        "endpoint": "https://httpbin.org/uuid",
        "params": {"name": "httpbin UUID", "auth": "none"},
    },
    {
        "type": "data/utility/user-agent",
        "endpoint": "https://httpbin.org/user-agent",
        "params": {"name": "httpbin User-Agent", "auth": "none"},
    },
    # GitHub
    {
        "type": "data/code/github-repos",
        "endpoint": "https://api.github.com/search/repositories",
        "params": {
            "name": "GitHub Search API",
            "auth": "optional-token",
            "docs": "https://docs.github.com/en/rest/search",
        },
    },
    {
        "type": "data/code/github-users",
        "endpoint": "https://api.github.com/users",
        "params": {"name": "GitHub Users API", "auth": "optional-token"},
    },
    # News
    {
        "type": "data/news/hacker-news",
        "endpoint": "https://hacker-news.firebaseio.com/v0",
        "params": {
            "name": "Hacker News API",
            "format": "json",
            "auth": "none",
            "docs": "https://github.com/HackerNews/API",
        },
    },
    # Text/NLP
    {
        "type": "data/text/dictionary",
        "endpoint": "https://api.dictionaryapi.dev/api/v2/entries/en",
        "params": {"name": "Free Dictionary API", "auth": "none"},
    },
    # Images
    {
        "type": "data/images/placeholder",
        "endpoint": "https://picsum.photos",
        "params": {
            "name": "Lorem Picsum",
            "auth": "none",
            "docs": "https://picsum.photos",
        },
    },
    # Space
    {
        "type": "data/science/iss-position",
        "endpoint": "http://api.open-notify.org/iss-now.json",
        "params": {"name": "ISS Position", "auth": "none"},
    },
    {
        "type": "data/science/astronomy-picture",
        "endpoint": "https://api.nasa.gov/planetary/apod",
        "params": {"name": "NASA APOD", "auth": "api_key=DEMO_KEY"},
    },
    # Country data
    {
        "type": "data/reference/countries",
        "endpoint": "https://restcountries.com/v3.1/all",
        "params": {
            "name": "REST Countries",
            "auth": "none",
            "docs": "https://restcountries.com",
        },
    },
    # Jokes/Fun (agents use these for personality)
    {
        "type": "data/fun/jokes",
        "endpoint": "https://official-joke-api.appspot.com/random_joke",
        "params": {"name": "Official Joke API", "auth": "none"},
    },
    {
        "type": "data/fun/cat-facts",
        "endpoint": "https://catfact.ninja/fact",
        "params": {"name": "Cat Facts", "auth": "none"},
    },
]
