FROM node:22-alpine
WORKDIR /app
COPY package.json ./
RUN npm install
COPY . .
EXPOSE 3002
HEALTHCHECK CMD wget -qO- http://127.0.0.1:3002 || exit 1
CMD ["node", "server.mjs"]
