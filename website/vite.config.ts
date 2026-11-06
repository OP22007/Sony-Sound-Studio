import { defineConfig } from 'vite';

const siteUrl = (process.env.SITE_URL || 'https://op22007.github.io/Sony-Sound-Studio/').replace(/\/?$/, '/');

export default defineConfig({
  base: './',
  server: { port: 4321, strictPort: true },
  preview: { port: 4322, strictPort: true },
  build: { target: 'es2022' },
  plugins: [{
    name: 'site-metadata',
    transformIndexHtml(html) {
      return html.replaceAll('https://op22007.github.io/Sony-Sound-Studio/', siteUrl);
    },
    generateBundle() {
      this.emitFile({ type: 'asset', fileName: 'robots.txt', source: `User-agent: *\nAllow: /\nSitemap: ${siteUrl}sitemap.xml\n` });
      this.emitFile({ type: 'asset', fileName: 'sitemap.xml', source: `<?xml version="1.0" encoding="UTF-8"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9"><url><loc>${siteUrl.replaceAll('&', '&amp;').replaceAll('<', '&lt;')}</loc></url></urlset>` });
    },
  }],
});
