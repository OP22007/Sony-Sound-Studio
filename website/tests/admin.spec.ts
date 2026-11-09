import { test, expect } from '@playwright/test';
import { readFileSync } from 'node:fs';
import { applyEdits } from '../../admin/server/edits.mjs';

const root = new URL('../../', import.meta.url);
const original = JSON.parse(readFileSync(new URL('docs/compatibility/devices.json', root), 'utf8'));
const dashboard = readFileSync(new URL('admin/views/dashboard.html', root), 'utf8');

async function workspace(page: any, rejectSave = false) {
  const writes: any[] = [];
  await page.route('https://studio-admin.test/**', async (route: any) => {
    const path = new URL(route.request().url()).pathname;
    if (path === '/') return route.fulfill({ contentType: 'text/html', body: dashboard });
    if (path.startsWith('/assets/')) return route.fulfill({ contentType: path.endsWith('.js') ? 'text/javascript' : 'text/css', body: readFileSync(new URL('admin/public' + path, root), 'utf8') });
    if (path === '/api/state') return route.fulfill({ json: { registry: structuredClone(original), appVersions:['0.1.3','0.1.4'], baseSha: 'source-head', pagesSha: 'pages-head', csrf: 'test-csrf', user: { login: 'OP22007' }, branch: 'codex/website-scroll-launch' } });
    if (path === '/api/save') {
      const data = route.request().postDataJSON(); writes.push(data);
      expect(route.request().headers()['x-csrf-token']).toBe('test-csrf');
      return rejectSave ? route.fulfill({ status: 409, json: { error: 'The source changed. Reload before saving.' } }) : route.fulfill({ json: { registry: applyEdits(original, data, 140067318), baseSha: 'saved-head', published: false } });
    }
    if (path === '/api/publish') {
      writes.push(route.request().postDataJSON());
      return route.fulfill({ json: { pagesSha: 'published-head', sourceSha: 'saved-head', deploymentRequested: true } });
    }
    return route.fulfill({ status: 404, json: {} });
  });
  await page.route('https://op22007.github.io/**', (route: any) => route.fulfill({ status: 204, body: '' }));
  await page.goto('https://studio-admin.test/');
  await expect(page.locator('#account')).toHaveText('OP22007 · owner');
  return writes;
}

async function stageResult(page: any) {
  await page.locator('#search').fill('WH-1000XM6');
  await page.locator('#models').getByRole('button', { name: 'WH-1000XM6', exact: true }).click();
  await page.locator('#features').getByRole('button', { name: /Noise control & transparency/ }).click();
  await page.locator('#status').selectOption('confirmed');
  await page.locator('#note').fill('Three modes and button tracking verified.');
  await page.locator('[data-platform="windows"]').click();
  await page.locator('#testedOn').fill('2026-01-01');
  await page.locator('#firmware').selectOption('3.1.5');
  await page.locator('#appVersion').selectOption('0.1.3');
  await page.getByRole('button', { name: 'Add result to review' }).click();
}

test('owner editor stages evidence, saves to GitHub and publishes only saved state', async ({ page }) => {
  const writes = await workspace(page);
  await expect(page.locator('#models button')).toHaveCount(92);
  await expect(page.locator('#save')).toBeDisabled();
  await stageResult(page);
  await expect(page.locator('#draft-count')).toHaveText('1 feature change');
  await expect(page.locator('#publish')).toBeDisabled();
  await page.locator('#save').click();
  await expect(page.locator('#notice')).toContainText('Saved to GitHub');
  expect(writes[0].changes[0]).toMatchObject({ platform: 'windows', feature: 'noise', status: 'confirmed', test: { firmware: '3.1.5' } });
  expect(writes[0].changes[0].test).toMatchObject({requested:'',readback:'',observed:'',steps:''});
  expect(writes[0].changes[0].test.osVersion).toBeUndefined();
  await expect(page.locator('#draft-count')).toHaveText('0 feature changes');
  await page.locator('#publish').click();
  await expect(page.locator('#notice')).toContainText('deployment requested');
  expect(writes[1]).toEqual({ baseSha: 'saved-head', pagesSha: 'pages-head' });
  await expect(page.locator('#publication')).toContainText('Deployment completion has not been verified');
});

test('failed save retains staged results and blocks publication', async ({ page }) => {
  await workspace(page, true); await stageResult(page); await page.locator('#save').click();
  await expect(page.locator('#notice')).toContainText('source changed');
  await expect(page.locator('#draft-count')).toHaveText('1 feature change');
  await expect(page.locator('#publish')).toBeDisabled();
});

test('confirmed status requires evidence and switching models protects drafts', async ({ page }) => {
  await workspace(page); await page.locator('#status').selectOption('confirmed');
  await page.getByRole('button', { name: 'Add result to review' }).click();
  await expect(page.locator('#draft-count')).toHaveText('0 feature changes');
  await expect(page.locator('#firmware')).toHaveAttribute('required', '');
  page.once('dialog', dialog => dialog.dismiss());
  const name = await page.locator('#model-name').textContent();
  await page.locator('#models button').nth(1).click();
  await expect(page.locator('#model-name')).toHaveText(name!);
});

test('owner editor fits mobile and desktop without horizontal overflow', async ({ page }) => {
  await workspace(page);
  for (const width of [390, 1440]) {
    await page.setViewportSize({ width, height: 900 });
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await expect(page.locator('#model-name')).toBeVisible();
  }
});
test('version dropdowns stay model-specific and support custom versions',async({page})=>{
  const writes=await workspace(page);await stageResult(page);
  await page.locator('#firmware').selectOption('__other__');await page.locator('#firmware-custom').fill('4.2.0');
  await page.locator('#appVersion').selectOption('__other__');await page.locator('#appVersion-custom').fill('codex-test-commit');
  await page.getByRole('button',{name:'Add result to review'}).click();
  await page.locator('#save').click();await expect(page.locator('#notice')).toContainText('Saved to GitHub');
  expect(writes[0].changes[0].test).toMatchObject({firmware:'4.2.0',appVersion:'codex-test-commit'});
  await expect(page.locator('#firmware option[value="4.2.0"]')).toHaveCount(1);
  await page.locator('#search').fill('WH-1000XM5');await page.locator('#models').getByRole('button',{name:'WH-1000XM5',exact:true}).click();
  await expect(page.locator('#firmware option[value="4.2.0"]')).toHaveCount(0);
  await expect(page.locator('#appVersion option[value="0.1.4"]')).toHaveCount(1);
});
test('platform defaults to the detected OS and can be overridden',async({page})=>{
  await page.addInitScript(()=>{
    Object.defineProperty(navigator,'userAgentData',{value:{platform:'macOS'},configurable:true});
  });
  await workspace(page);
  await expect(page.locator('[data-platform="macos"]')).toHaveAttribute('aria-pressed','true');
  await page.locator('[data-platform="linux"]').click();
  await expect(page.locator('[data-platform="linux"]')).toHaveAttribute('aria-pressed','true');
  await expect(page.locator('#osVersion')).toHaveCount(0);
});
