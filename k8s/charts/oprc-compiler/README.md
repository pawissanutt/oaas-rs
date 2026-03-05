# oprc-compiler Helm Chart

Deploys the OaaS Compiler Service — compiles TypeScript source code into WASM Components for in-process execution in ODGM.

## Usage

Deploy alongside the PM:

```bash
# Using deploy.sh with --compiler flag
./k8s/charts/deploy.sh deploy --compiler

# Or standalone
helm install oaas-compiler ./k8s/charts/oprc-compiler -n oaas
```

## Configuration

| Parameter | Description | Default |
|-----------|-------------|---------|
| `image.repository` | Container image | `ghcr.io/pawissanutt/oaas-rs/compiler` |
| `image.tag` | Image tag | `latest` |
| `service.port` | Service port | `3000` |
| `config.maxSourceSize` | Max source code bytes | `1048576` (1 MB) |
| `config.compileTimeoutMs` | Compilation timeout | `120000` (2 min) |
| `config.maxOldSpaceSize` | Node.js heap limit (MB) | `1024` |
| `config.logLevel` | Log level | `info` |

## Integration with PM

When deployed, configure PM to use the compiler by setting:

```yaml
config:
  compiler:
    url: "http://oaas-compiler-oprc-compiler.oaas.svc.cluster.local:3000"
```

Or use the `--compiler` flag with `deploy.sh` which auto-configures this.
