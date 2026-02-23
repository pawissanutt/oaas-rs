export * from "./bindings/OPackage";
export * from "./bindings/OClass";
export * from "./bindings/OFunction";
export * from "./bindings/FunctionType";

export * from "./bindings/OClassDeployment";

export interface ClusterInfo {
    name: string;
    status: "Healthy" | "Degraded" | "Unhealthy";
    crmVersion?: string;
    nodes?: string; // "3/3 ready"
    avail?: string; // "99.9%"
    lastSeen?: string;
}

export interface ObjEntry {
    [key: string]: unknown;
}

export interface ObjEvent {
    type: string;
    target?: string;
}

export interface OObject {
    id: string;
    class: string;
    partition: number;
    entries: ObjEntry;
    events: ObjEvent[];
}

export interface TopologyNode {
    data: {
        id: string;
        type: "Package" | "Class" | "Function" | "Environment" | "Router" | "Gateway" | "ODGM";
        label: string;
        parent?: string;
        status?: string;
        metadata?: Record<string, string>;
    };
}

export interface TopologyEdge {
    data: {
        source: string;
        target: string;
        label?: string;
    };
}
