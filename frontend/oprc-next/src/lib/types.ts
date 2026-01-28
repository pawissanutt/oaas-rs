export interface OPackage {
    name: string;
    version?: string;
    classes: ClassRuntime[];
    functions: FunctionDef[];
    description?: string;
}

export interface ClassRuntime {
    name: string; // The class name, e.g. "EchoClass"
    package_name?: string;
    partitions: number;
    functions: string[]; // List of function names
}

export interface FunctionDef {
    name: string;
    type: "Builtin" | "Custom" | "Macro" | "Logical";
    description?: string;
    boundTo?: string; // e.g. "ClassName.fnName"
}

export interface OClassDeployment {
    key: string; // e.g. "package.ClassName"
    package: string;
    class: string;
    status: "Running" | "Deploying" | "Pending" | "Error" | "Down";
    selectedEnvs: string[];
    lastReconciled?: string; // relative time or ISO
    error?: string;
    nfr?: {
        throughput?: string;
        availability?: string;
        cpu?: string;
    };
}

export interface ClusterInfo {
    name: string;
    status: "Healthy" | "Degraded" | "Unhealthy";
    crmVersion?: string;
    nodes?: string; // "3/3 ready"
    avail?: string; // "99.9%"
    lastSeen?: string;
}

export interface ObjEntry {
    [key: string]: any;
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
