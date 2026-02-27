# oprc-compiler — OaaS TypeScript-to-WASM Compiler Service
#
# Build context: repository root (tools/oprc-compiler/../../)
# Usage:
#   docker build -f tools/oprc-compiler/Dockerfile -t oprc-compiler .

FROM node:20-slim AS builder

WORKDIR /app

# Copy package files for dependency installation
COPY tools/oprc-compiler/package.json tools/oprc-compiler/package-lock.json* ./

# Install all dependencies (including dev for building)
RUN npm install

# Copy compiler source
COPY tools/oprc-compiler/tsconfig.json ./
COPY tools/oprc-compiler/src/ ./src/

# Build TypeScript
RUN npm run build

# ─── Production Stage ───────────────────────────────────────

FROM node:20-slim

WORKDIR /app

# Copy package files
COPY tools/oprc-compiler/package.json tools/oprc-compiler/package-lock.json* ./

# Install production dependencies only
RUN npm install --omit=dev

# Copy built JavaScript from builder
COPY --from=builder /app/dist/ ./dist/

# Copy WIT files from repository
COPY data-plane/oprc-wasm/wit/ ./wit/

# Copy SDK TypeScript source (needed at runtime for bundling user code)
COPY tools/oaas-sdk-ts/src/ ./sdk-src/

# Environment defaults
ENV PORT=3000
ENV HOST=0.0.0.0
ENV WIT_PATH=/app/wit
ENV SDK_PATH=/app/sdk-src
ENV MAX_SOURCE_SIZE=52428800
ENV COMPILE_TIMEOUT_MS=120000
ENV NODE_OPTIONS="--max-old-space-size=1024"

EXPOSE 3000

CMD ["node", "dist/index.js"]
