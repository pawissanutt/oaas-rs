import { OPackage } from "./bindings/OPackage";
import { OClassDeployment } from "./bindings/OClassDeployment";
import { OObject } from "./types";
import { ClusterInfo } from "./types";

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "";
const API_V1 = `${API_BASE}/api/v1`;

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

export async function fetchEnvironments(): Promise<ClusterInfo[]> {
    try {
        // Use the envs endpoint or topology
        const res = await fetch(`${API_V1}/envs`);
        if (!res.ok) throw new Error(`Failed to fetch environments: ${res.status}`);
        const data = await res.json();
        // oprc-gui models environments as simple strings or objects, map accordingly
        // Assuming the API returns a list of environment names or statuses
        // We might need to adjust types based on actual response
        return data.map((env: string | { name: string }) => ({
            name: typeof env === 'string' ? env : env.name,
            status: "Healthy", // Mock status if not provided
        }));
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
