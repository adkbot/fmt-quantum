FROM node:22-alpine
WORKDIR /app

COPY package*.json ./
RUN npm install
COPY tsconfig.json ./
COPY src ./src

ENV NODE_ENV=production
EXPOSE 8090
CMD ["npm", "start"]
