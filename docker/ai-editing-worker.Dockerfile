FROM node:22-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
	&& rm -rf /var/lib/apt/lists/*
RUN npm install -g bun@1.3.5

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

WORKDIR /opt/ai-editing-worker
COPY package.json bun.lock bunfig.toml ./
COPY apps/web/package.json apps/web/package.json
COPY apps/web/patches/solid-gesture@0.0.3.patch apps/web/patches/@thisbeyond%2Fsolid-dnd@0.7.5.patch apps/web/patches/@lexical%2Fmarkdown@0.45.0.patch apps/web/patches/@kobalte%2Fcore@0.13.11.patch apps/web/patches/virtua@0.48.8.patch apps/web/patches/@tanstack%2Fsolid-query@5.90.3.patch apps/web/patches/@lexical%2Ftable@0.45.0.patch apps/web/patches/ai-fallback@2.0.1.patch apps/web/patches/
COPY packages/observability/package.json packages/observability/package.json
COPY packages/collaboration/package.json packages/collaboration/package.json
COPY packages/loro-mirror/package.json packages/loro-mirror/package.json
COPY packages/lexical-core/package.json packages/lexical-core/package.json
COPY services/ai-editing-worker/package.json services/ai-editing-worker/package.json
COPY services/bots/anthropic-status-bot/package.json services/bots/anthropic-status-bot/package.json
COPY services/bots/stripe-payment-bot/package.json services/bots/stripe-payment-bot/package.json
COPY services/lexical-service/package.json services/lexical-service/package.json
RUN bun install --frozen-lockfile --filter ai-editing-worker --ignore-scripts \
	&& node -e 'const fs=require("fs"); const workerd=require("workerd/package.json"); const wrangler=require("wrangler/package.json"); if (wrangler.version!=="4.110.0" || workerd.version!=="1.20260708.1") process.exit(1); const p=require.resolve("@cloudflare/workerd-linux-arm64/bin/workerd"); const b=fs.readFileSync(p); if ((fs.statSync(p).mode&0o111)===0 || b[0]!==0x7f || b[1]!==0x45 || b[2]!==0x4c || b[3]!==0x46 || b.readUInt16LE(18)!==183) process.exit(1); try { require.resolve("@cloudflare/workerd-darwin-arm64/bin/workerd"); process.exit(1); } catch (error) { if (error.code!=="MODULE_NOT_FOUND") throw error; }'

COPY packages/observability packages/observability
COPY packages/collaboration packages/collaboration
COPY packages/loro-mirror packages/loro-mirror
COPY packages/lexical-core packages/lexical-core
COPY services/ai-editing-worker services/ai-editing-worker

WORKDIR /opt/ai-editing-worker/services/ai-editing-worker
EXPOSE 8933

CMD ["sh", "-c", "\
  bun scripts/generate-sandbox.ts && \
  printf 'OPENAI_API_KEY=%s\\nANTHROPIC_API_KEY=%s\\nCEREBRAS_API_KEY=%s\\n' \
    \"${OPENAI_API_KEY}\" \
    \"${ANTHROPIC_API_KEY}\" \
    \"${CEREBRAS_API_KEY}\" \
    > .dev.vars && \
  /opt/ai-editing-worker/node_modules/.bin/wrangler dev \
    --env local \
    --ip 0.0.0.0 \
    --persist-to /app/services/ai-editing-worker/.wrangler/state \
    --var SYNC_WS_BASE:ws://sync-service:8787\
"]
