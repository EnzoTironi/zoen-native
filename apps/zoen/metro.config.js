const path = require('node:path');
const { getDefaultConfig } = require('expo/metro-config');
const { webIsolationHeaders } = require('./config/web-isolation.cjs');

const config = getDefaultConfig(__dirname);
config.watchFolders = [path.resolve(__dirname, '../../packages/zoen-ui')];
config.resolver.nodeModulesPaths = [path.resolve(__dirname, 'node_modules')];
config.server.enhanceMiddleware = (middleware) => (request, response, next) => {
  for (const [header, value] of Object.entries(webIsolationHeaders(request.url))) {
    response.setHeader(header, value);
  }
  middleware(request, response, next);
};
module.exports = config;
