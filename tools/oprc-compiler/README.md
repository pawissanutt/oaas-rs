# oprc-compiler

OaaS Compiler Service — compiles TypeScript source code into WASM Components for the OaaS runtime.

This microservice accepts TypeScript via a REST API, bundles it with the `@oaas/sdk`, and produces a [WASM Component](https://component-model.bytecodealliance.org/) targeting the `oaas-object` WIT world. It is designed to be called by the Package Manager (`oprc-pm`) during class deployment.

## Quick Start

### Prerequisites

- **Node.js** 20+
- **npm** 9+
- The `@oaas/sdk` TypeScript source at `../oaas-sdk-ts/src/`
- The WIT definitions at `../../data-plane/oprc-wasm/wit/`

### Install & Run

```bash
cd tools/oprc-compiler
npm install
npm run dev
```

The server starts on `http://localhost:3000` by default.

### Compile a TypeScript Service

```bash
curl -X POST http://localhost:3000/compile \
  -H "Content-Type: application/json" \
  -d '{
    "source": "import { OaaSService, OaaSFunction } from \"@oaas/sdk\";\n\n@OaaSService()\nexport default class Counter {\n  count: number = 0;\n\n  @OaaSFunction()\n  increment(): void { this.count++; }\n}",
    "language": "typescript"
  }' \
  --output counter.wasm
```

On success, the response is the raw WASM binary (`application/wasm`, HTTP 200).

## API Reference

### `POST /compile`

Compile TypeScript source into a WASM Component.

**Request Body** (JSON):

| Field      | Type   | Required | Description                                           |
|------------|--------|----------|-------------------------------------------------------|
| `source`   | string | ✓        | TypeScript source code (single module, default export) |
| `language` | string | ✓        | Must be `"typescript"`                                |

**Success Response** — `200 OK`

- Content-Type: `application/wasm`
- Body: raw WASM Component binary (~11 MB, includes embedded SpiderMonkey engine)

**Error Response** — `400 Bad Request`

```json
{
  "success": false,
  "errors": ["source.ts:5:10: ')' expected."]
}
```

**Error Response** — `413 Payload Too Large`

```json
{
  "success": false,
  "errors": ["Source code exceeds maximum size of 1048576 bytes"]
}
```

### `GET /health`

Health check endpoint.

**Response** — `200 OK`

```json
{ "status": "ok" }
```

## Configuration

All settings are configured via environment variables:

| Variable             | Default                               | Description                        |
|----------------------|---------------------------------------|------------------------------------|
| `PORT`               | `3000`                                | HTTP port                          |
| `HOST`               | `0.0.0.0`                             | Bind address                       |
| `WIT_PATH`           | `../../data-plane/oprc-wasm/wit`      | Path to WIT directory              |
| `SDK_PATH`           | `../oaas-sdk-ts/src`                  | Path to `@oaas/sdk` source         |
| `MAX_SOURCE_SIZE`    | `1048576` (1 MB)                      | Maximum source size in bytes       |
| `COMPILE_TIMEOUT_MS` | `120000` (2 min)                      | Per-compilation timeout            |
| `LOG_LEVEL`          | `info`                                | Fastify log level                  |

## Docker

Build from the repository root:

```bash
docker build -f tools/oprc-compiler/Dockerfile -t oprc-compiler .
```

Run:

```bash
docker run -p 3000:3000 oprc-compiler
```

The Docker image bundles the WIT definitions and SDK source so no external paths are needed.

## Docker Compose

The service is defined in `docker-compose.dev.yml`:

```bash
docker compose -f docker-compose.dev.yml up oprc-compiler
```

## NPM Scripts

| Script            | Description                          |
|-------------------|--------------------------------------|
| `npm run build`   | Compile TypeScript → `dist/`         |
| `npm start`       | Run compiled server (`dist/index.js`)|
| `npm run dev`     | Run from source via ts-node          |
| `npm test`        | Run test suite (vitest)              |
| `npm run test:watch` | Run tests in watch mode           |
| `npm run test:compile` | Manual compilation test          |
| `npm run clean`   | Remove `dist/` directory             |

## How It Works

See [DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md) for architecture details and the full compilation pipeline.

## Writing TypeScript Services

See [USER_GUIDE.md](docs/USER_GUIDE.md) for how to write TypeScript services that compile through this service.
