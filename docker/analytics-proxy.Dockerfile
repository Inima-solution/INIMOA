FROM node:22-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
	&& rm -rf /var/lib/apt/lists/*
RUN npm install -g bun@1.3.5

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

WORKDIR /opt/analytics-proxy
COPY services/analytics-proxy/package.json services/analytics-proxy/bun.lock ./
RUN bun install --frozen-lockfile --ignore-scripts \
	&& node -e 'const fs=require("fs"); const workerd=require("workerd/package.json"); const wrangler=require("wrangler/package.json"); if (wrangler.version!=="4.74.0" || workerd.version!=="1.20260312.1") process.exit(1); const p=require.resolve("@cloudflare/workerd-linux-arm64/bin/workerd"); const b=fs.readFileSync(p); if ((fs.statSync(p).mode&0o111)===0 || b[0]!==0x7f || b[1]!==0x45 || b[2]!==0x4c || b[3]!==0x46 || b.readUInt16LE(18)!==183) process.exit(1); try { require.resolve("@cloudflare/workerd-darwin-arm64/bin/workerd"); process.exit(1); } catch (error) { if (error.code!=="MODULE_NOT_FOUND") throw error; }'

COPY services/analytics-proxy/src src
COPY services/analytics-proxy/wrangler.jsonc ./

WORKDIR /opt/analytics-proxy
EXPOSE 8098

# Wrangler and worker dependencies are image-private. Forward OTLP (both
# signals) to the local collector (--var overrides the wrangler.jsonc host
# defaults); no DD_API_KEY locally, so the worker skips key injection.
CMD ["sh", "-c", "\
  /opt/analytics-proxy/node_modules/.bin/wrangler dev \
    --env local \
    --ip 0.0.0.0 \
    --port 8098 \
    --var OTLP_TRACES_INTAKE_URL:http://otel-collector:4318 \
    --var OTLP_LOGS_INTAKE_URL:http://otel-collector:4318\
"]
