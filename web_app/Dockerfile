FROM node:19-alpine AS dependencies
RUN apk add --no-cache libc6-compat
WORKDIR /app
COPY package.json yarn.lock* package-lock.json* pnpm-lock.yaml* ./
RUN npm install
RUN npm ci --only=production

# Rebuild the source code only when needed
FROM node:19-alpine AS builder
WORKDIR /app
COPY --from=dependencies /app/package.json ./package.json
COPY --from=dependencies /app/node_modules ./node_modules
COPY . .

RUN chown -R node:node .
USER node

RUN npm run build

EXPOSE 8080
ENV PORT 8080

# CMD ["node", "server.js"]
CMD ["npm", "run", "start"]