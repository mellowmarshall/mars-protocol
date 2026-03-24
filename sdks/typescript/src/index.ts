/**
 * mars-protocol — TypeScript client for the Mesh Agent Routing Standard.
 *
 * A thin wrapper around the mesh-gateway HTTP API that lets you publish
 * and discover capabilities on the decentralized mesh network.
 *
 * @example
 * ```ts
 * const client = new MeshClient("http://localhost:3000");
 * await client.publish("compute/inference/text-generation", {
 *   endpoint: "https://my-agent.example.com/v1/generate",
 *   params: { model: "llama-3.3-70b" },
 * });
 * const results = await client.discover("compute/inference");
 * ```
 */

// ── Types ─────────────────────────────────────────────────────────────

export interface Descriptor {
  id: string;
  publisher: string;
  type: string;
  endpoint: string;
  params?: Record<string, unknown>;
  timestamp: number;
  ttl: number;
  sequence: number;
}

export interface PublishResult {
  ok: boolean;
  descriptor_id: string;
}

export interface HealthStatus {
  status: string;
  identity: string;
  seed: string;
}

export interface PublishOptions {
  endpoint: string;
  params?: Record<string, unknown>;
}

export class MeshError extends Error {
  public readonly statusCode: number;
  public readonly body: string;

  constructor(statusCode: number, body: string) {
    super(`MeshError ${statusCode}: ${body}`);
    this.name = "MeshError";
    this.statusCode = statusCode;
    this.body = body;
  }
}

// ── Client ────────────────────────────────────────────────────────────

export class MeshClient {
  private readonly baseUrl: string;
  private readonly timeout: number;

  /**
   * Create a new MARS mesh client.
   *
   * @param gatewayUrl - Base URL of the mesh-gateway (e.g. "http://localhost:3000")
   * @param timeout - Request timeout in milliseconds (default: 30000)
   */
  constructor(gatewayUrl: string, timeout = 30_000) {
    // Strip trailing slash
    this.baseUrl = gatewayUrl.replace(/\/+$/, "");
    this.timeout = timeout;
  }

  /**
   * Publish a capability to the mesh.
   *
   * @param capabilityType - Hierarchical type (e.g. "compute/inference/text-generation")
   * @param options - Endpoint URL and optional params
   * @returns The publish result with descriptor ID
   */
  async publish(
    capabilityType: string,
    options: PublishOptions
  ): Promise<PublishResult> {
    const body = {
      type: capabilityType,
      endpoint: options.endpoint,
      ...(options.params !== undefined && { params: options.params }),
    };

    const response = await this.fetch("/v1/publish", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });

    return response as PublishResult;
  }

  /**
   * Discover capabilities on the mesh.
   *
   * @param capabilityType - Type prefix to search for (e.g. "compute/inference")
   * @returns Array of matching descriptors
   */
  async discover(capabilityType: string): Promise<Descriptor[]> {
    const params = new URLSearchParams({ type: capabilityType });
    const response = (await this.fetch(
      `/v1/discover?${params.toString()}`,
      { method: "GET" }
    )) as { descriptors: Descriptor[] };

    return response.descriptors;
  }

  /**
   * Check the gateway's health and identity.
   */
  async health(): Promise<HealthStatus> {
    return (await this.fetch("/health", { method: "GET" })) as HealthStatus;
  }

  // ── Internal ──────────────────────────────────────────────────────

  private async fetch(
    path: string,
    init: RequestInit
  ): Promise<unknown> {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);

    try {
      const response = await globalThis.fetch(`${this.baseUrl}${path}`, {
        ...init,
        signal: controller.signal,
      });

      const text = await response.text();

      if (!response.ok) {
        throw new MeshError(response.status, text);
      }

      return JSON.parse(text);
    } finally {
      clearTimeout(timer);
    }
  }
}
