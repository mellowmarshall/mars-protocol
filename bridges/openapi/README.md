# MARS OpenAPI Bridge

Register any REST API on the mesh by pointing at its OpenAPI/Swagger spec.

## Usage

```bash
pip install httpx pyyaml

# Register all endpoints from a public API spec
python openapi_bridge.py --gateway http://localhost:3000 \
    --spec https://petstore.swagger.io/v2/swagger.json

# With a type prefix for organization
python openapi_bridge.py --gateway http://localhost:3000 \
    --spec https://api.stripe.com/openapi.json \
    --prefix "compute/payments"

# From a local file
python openapi_bridge.py --gateway http://localhost:3000 \
    --spec ./my-api.yaml

# Preview without publishing
python openapi_bridge.py --dry-run --spec https://api.example.com/openapi.json

# Only register GET endpoints
python openapi_bridge.py --gateway http://localhost:3000 \
    --spec ./api.yaml --methods GET
```

Each endpoint in the spec becomes a mesh descriptor. Agents discover them with:

```python
from mesh_protocol import MeshClient
client = MeshClient("http://localhost:3000")
results = client.discover("api/pets/get")  # auto-inferred type
```

## Supports

- OpenAPI 3.x (JSON or YAML)
- Swagger 2.0 (JSON or YAML)
- Remote URLs and local files
- Custom type prefixes for organization
- Method filtering
