import compatibility from '../docs/compatibility/devices.json';
import { defineConfig } from 'vite';

const siteUrl = (process.env.SITE_URL || 'https://op22007.github.io/Sony-Sound-Studio/').replace(/\/?$/, '/');

export default defineConfig({
  base: './',
  server: { port: 4321, strictPort: true },
  preview: { port: 4322, strictPort: true },
  build: { target: 'es2022' },
  plugins: [{
    name: 'site-metadata',
    configureServer(server) {
      // Public HTML directories need an explicit index during Vite development.
      server.middlewares.use((request, _response, next) => {
        if (request.url && /^\/devices\/(?:[a-z0-9-]+\/)?(?:\?.*)?$/.test(request.url)) {
          const [path, query] = request.url.split('?');
          request.url = `${path}index.html${query ? '?' + query : ''}`;
        }
        next();
      });
    },
    transformIndexHtml(html) {
      return html.replaceAll('https://op22007.github.io/Sony-Sound-Studio/', siteUrl);
    },
    generateBundle() {
      this.emitFile({ type: 'asset', fileName: 'robots.txt', source: `User-agent: *\nAllow: /\nSitemap: ${siteUrl}sitemap.xml\n` });
      this.emitFile({ type: 'asset', fileName: 'sitemap.xml', source: `<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><url><loc>${[siteUrl, siteUrl + "devices/", ...compatibility.devices.map(device => siteUrl + "devices/" + device.id + "/")].map(url => url.replaceAll("&", "&amp;").replaceAll("<", "&lt;")).join("</loc></url><url><loc>")}</loc></url></urlset>` });
    },
  }],
});
