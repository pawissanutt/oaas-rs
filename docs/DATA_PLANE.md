# Data Plane Architecture

The Data Plane handles the actual execution of functions, routing of requests, and management of stateful objects. It is designed for high performance, low latency, and scalability.

## Components

### 1. Gateway
**Location:** `data-plane/oprc-gateway`

The Gateway is the ingress point for the Data Plane. It accepts external requests (REST/gRPC) and translates them into internal Zenoh messages.

*   **Stateless**: Can be scaled horizontally.
*   **Protocol Translation**: Converts HTTP/gRPC invocation requests into Zenoh RPC calls.
*   **Routing**: Forwards requests to the appropriate Router or directly to the mesh.

### 2. Router
**Location:** `data-plane/oprc-router`

The Router manages the Zenoh-based communication mesh.

*   **Message Broker**: Handles Pub/Sub and RPC traffic between Gateway, Functions, and ODGM.
*   **Service Discovery**: Helps components discover each other within the mesh.

### 3. Object Data Grid Manager (ODGM)
**Location:** `data-plane/oprc-odgm`

ODGM is a distributed, stateful object store that runs alongside functions. Unlike a traditional centralized database, ODGM is often deployed per-class or per-tenant to ensure isolation and scalability.

*   **Distributed Storage**: Manages object state using a Raft-based consensus algorithm.
*   **Granular Storage**: Stores object fields individually for efficient partial updates.
*   **Event Pipeline**: Emits events (V2 Pipeline) when objects are created, updated, or deleted.
*   **String IDs**: Supports human-readable string identifiers for objects.

For deep technical details on ODGM features, see [ODGM Features & Internals](ODGM_FEATURES.md).

### 4. Function Runtimes
**Location:** User-defined (managed by CRM)

These are the actual containers running user code.

*   **Invocation**: Triggered via Zenoh RPC (from Gateway or other functions).
*   **State Access**: Functions access state in ODGM via the Zenoh mesh.
*   **Discovery**: Functions are injected with ODGM connection details by the CRM.

## Data Flow

### Function Invocation
1.  **Client** sends an invocation request to **Gateway** (e.g., `POST /api/v1/invoke`).
2.  **Gateway** translates the request to a Zenoh RPC call targeting the specific function topic.
3.  **Router** (or the mesh directly) routes the message to an available **Function Runtime** instance.
4.  **Function** executes the logic.
    *   If it needs state, it queries **ODGM** via Zenoh.
    *   If it modifies state, it sends updates to **ODGM**.
5.  **Function** returns the result via Zenoh.
6.  **Gateway** translates the result back to HTTP/gRPC and responds to the **Client**.

### Object Access
1.  **Client** requests object data via **Gateway**.
2.  **Gateway** forwards the request to **ODGM**.
3.  **ODGM** retrieves the data (from memory or disk) and returns it.
