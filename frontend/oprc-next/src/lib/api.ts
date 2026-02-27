import { OPackage } from "./bindings/OPackage";
import { OClassDeployment } from "./bindings/OClassDeployment";
import { OObject } from "./types";
import { ClusterInfo } from "./types";

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "";
const API_V1 = `${API_BASE}/api/v1`;

// ---------------------------------------------------------------------------
// Error helper
// ---------------------------------------------------------------------------
export class ApiError extends Error {
    constructor(public status: number, message: string) {
        super(message);
        this.name = "ApiError";
    }
}

async function throwIfNotOk(res: Response, action: string) {
    if (!res.ok) {
        let msg = `${action} failed: ${res.status}`;
        try {
            const body = await res.json();
            if (body.error) msg = body.error;
        } catch { /* ignore parse errors */ }
        throw new ApiError(res.status, msg);
    }
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------
export interface HealthResponse {
    status: "healthy" | "unhealthy";
    service: string;
    version: string;
    timestamp: string;
    storage: { status: string; message?: string };
}

export async function fetchHealth(): Promise<HealthResponse> {
    const res = await fetch(`${API_BASE}/health`);
    return await res.json();
}

// ---------------------------------------------------------------------------
// Topology
// ---------------------------------------------------------------------------
export interface TopologySnapshot {
    nodes: Array<{
        id: string;
        node_type: string;
        status?: string;
        metadata?: Record<string, string>;
        deployed_classes?: string[];
    }>;
    edges: Array<{
        from_id: string;
        to_id: string;
        metadata?: Record<string, string>;
    }>;
    timestamp?: { seconds: number; nanos: number };
}

export async function fetchTopology(source: "deployments" | "zenoh" = "deployments"): Promise<TopologySnapshot> {
    const res = await fetch(`${API_V1}/topology?source=${source}`);
    if (!res.ok) throw new ApiError(res.status, `Failed to fetch topology: ${res.status}`);
    return await res.json();
}

export async function fetchPackages(): Promise<OPackage[]> {
    try {
        const res = await fetch(`${API_V1}/packages`);
        if (!res.ok) throw new Error(`Failed to fetch packages: ${res.status}`);
        return await res.json();
    } catch (e) {
        console.error(e);
        return [];
    }
}

export async function fetchPackage(name: string): Promise<OPackage> {
    const res = await fetch(`${API_V1}/packages/${encodeURIComponent(name)}`);
    await throwIfNotOk(res, "Fetch package");
    return await res.json();
}

export async function createPackage(pkg: OPackage): Promise<{ id: string; status: string; message?: string }> {
    const res = await fetch(`${API_V1}/packages`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(pkg),
    });
    await throwIfNotOk(res, "Create package");
    return await res.json();
}

export async function deletePackage(name: string): Promise<void> {
    const res = await fetch(`${API_V1}/packages/${encodeURIComponent(name)}`, {
        method: "DELETE",
    });
    await throwIfNotOk(res, "Delete package");
}

export async function fetchDeployments(): Promise<OClassDeployment[]> {
    try {
        const res = await fetch(`${API_V1}/deployments`);
        if (!res.ok) throw new Error(`Failed to fetch deployments: ${res.status}`);
        return await res.json();
    } catch (e) {
        console.error(e);
        return [];
    }
}

export async function createDeployment(dep: Partial<OClassDeployment>): Promise<{ id: string; status: string; message?: string }> {
    const res = await fetch(`${API_V1}/deployments`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(dep),
    });
    await throwIfNotOk(res, "Create deployment");
    return await res.json();
}

export async function deleteDeployment(key: string): Promise<{ message: string; id: string; deleted_envs: string[] }> {
    const res = await fetch(`${API_V1}/deployments/${encodeURIComponent(key)}`, {
        method: "DELETE",
    });
    await throwIfNotOk(res, "Delete deployment");
    return await res.json();
}

export async function fetchEnvironments(): Promise<ClusterInfo[]> {
    try {
        const res = await fetch(`${API_V1}/envs`);
        if (!res.ok) throw new Error(`Failed to fetch environments: ${res.status}`);
        const data = await res.json();

        return data.map((env: any) => {
            const h = env.health || {};
            return {
                name: typeof env === 'string' ? env : (env.name || "Unknown"),
                status: h.status || "Healthy",
                crmVersion: h.crm_version,
                nodes: typeof h.ready_nodes === 'number' && typeof h.node_count === 'number'
                    ? `${h.ready_nodes}/${h.node_count} ready`
                    : undefined,
                avail: typeof h.availability === 'number'
                    ? `${(h.availability * 100).toFixed(1)}%`
                    : undefined,
                lastSeen: h.last_seen ? new Date(h.last_seen).toLocaleString() : undefined,
                raw: env,
            };
        });
    } catch (e) {
        console.error(e);
        return [];
    }
}

export async function fetchObjects(classKey: string, partition: number): Promise<OObject[]> {
    try {
        const res = await fetch(`${API_BASE}/api/gateway/api/class/${classKey}/${partition}/objects`);
        if (!res.ok) throw new Error(`Failed to fetch objects: ${res.status}`);
        const data = await res.json();
        return data.objects || [];
    } catch (e) {
        console.error(e);
        return [];
    }
}

export async function fetchObject(classKey: string, partition: number, objectId: string): Promise<unknown> {
    const res = await fetch(
        `${API_BASE}/api/gateway/api/class/${classKey}/${partition}/objects/${encodeURIComponent(objectId)}`,
        { headers: { "Accept": "application/json" } }
    );
    await throwIfNotOk(res, "Fetch object");
    return await res.json();
}

export async function createOrUpdateObject(
    classKey: string,
    partition: number,
    objectId: string,
    body: unknown
): Promise<void> {
    const res = await fetch(
        `${API_BASE}/api/gateway/api/class/${classKey}/${partition}/objects/${encodeURIComponent(objectId)}`,
        {
            method: "PUT",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(body),
        }
    );
    await throwIfNotOk(res, "Create/update object");
}

export async function deleteObject(classKey: string, partition: number, objectId: string): Promise<void> {
    const res = await fetch(
        `${API_BASE}/api/gateway/api/class/${classKey}/${partition}/objects/${encodeURIComponent(objectId)}`,
        { method: "DELETE" }
    );
    await throwIfNotOk(res, "Delete object");
}

export async function invokeFunction(
    classKey: string,
    partition: number,
    objectId: string,
    functionName: string,
    payload: unknown
): Promise<unknown> {
    try {
        const res = await fetch(`${API_BASE}/api/gateway/api/class/${classKey}/${partition}/${objectId}/${functionName}`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload),
        });
        if (!res.ok) {
            const text = await res.text();
            throw new Error(`Invocation failed: ${res.status} ${text}`);
        }
        return await res.json();
    } catch (e) {
        console.error(e);
        throw e;
    }
}

// Helper to use with SWR
export const fetcher = (url: string) => {
    // Check if url starts with /, prepend API_BASE if needed, but SWR might use full URL
    // If we use relative URLs in SWR keys, we need to handle it.
    // For now, assume keys are relative to API_V1 or API_BASE?
    // Let's make fetcher robust.
    const fullUrl = url.startsWith("http") ? url : `${API_BASE}${url}`;
    return fetch(fullUrl).then((res) => res.json());
};
