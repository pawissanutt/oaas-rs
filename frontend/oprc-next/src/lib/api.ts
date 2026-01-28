import { OPackage, OClassDeployment, ClusterInfo, OObject } from "./types";

const API_BASE = "/api/v1";

export async function fetchPackages(): Promise<OPackage[]> {
    // In real app: return fetch(`${API_BASE}/packages`).then(res => res.json());
    return [];
}

export async function fetchDeployments(): Promise<OClassDeployment[]> {
    // In real app: return fetch(`${API_BASE}/deployments`).then(res => res.json());
    return [];
}

export async function fetchEnvironments(): Promise<ClusterInfo[]> {
    // In real app: return fetch(`${API_BASE}/envs`).then(res => res.json());
    return [];
}

export async function fetchObjects(classKey: string, partition: number): Promise<OObject[]> {
    // In real app: return fetch(`/api/gateway/api/class/${classKey}/${partition}/objects`).then(res => res.json());
    return [];
}

// Helper to use with SWR
export const fetcher = (url: string) => fetch(url).then((res) => res.json());
