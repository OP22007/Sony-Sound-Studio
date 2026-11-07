import { readFile } from 'node:fs/promises';
import { config, HttpError, random, challenge, equal, seal, unseal, cookie, readCookie, checkMutation, securityHeaders } from './security.mjs';
import { githubClient, requireOwner, readSnapshot, readJSON, commitFiles } from './github.mjs';
import { applyEdits, testJournal } from './edits.mjs';
import { generateCompatibility } from '../lib/compatibility.mjs';

const json = (res, status, value) => { res.statusCode=status; res.setHeader('Content-Type','application/json; charset=utf-8'); res.end(JSON.stringify(value)); };
const redirect = (res, url) => {res.statusCode=303; res.setHeader('Location',url); res.end();};
// Only expose known error codes and our own guidance, never upstream descriptions.
function authorizationError(code) {
  const messages={
    incorrect_client_credentials:'GitHub rejected the configured Client ID or Client Secret. Check that both belong to the same GitHub App in Vercel Production, then redeploy.',
    bad_verification_code:'The GitHub sign-in code expired, was already used, or did not match this request. Start a fresh sign-in from /auth/login; do not refresh the callback page.',
    redirect_uri_mismatch:'The GitHub App callback URL must be https://sound-studio-admin.vercel.app/auth/callback.',
    pkce_verification_failed:'GitHub could not verify this sign-in session. Start a fresh sign-in from /auth/login in one browser tab.',
    unverified_user_email:'Verify your GitHub account email address, then start sign-in again.',
    access_denied:'GitHub authorization was declined. Start sign-in again and authorize the app with OP22007.',
    application_suspended:'This GitHub App is suspended. Check its GitHub settings before signing in again.',
  };
  return Object.hasOwn(messages,code) ? messages[code] : 'GitHub did not authorize sign-in. Start again from /auth/login. If it repeats, check the GitHub App credentials and installation.';
}
async function body(req) {
  if (Number(req.headers['content-length']) > 150000) throw new HttpError(413, 'Request is too large.');
  if (req.body !== undefined) {
    if (Buffer.byteLength(typeof req.body === 'string' ? req.body : JSON.stringify(req.body)) > 150000) throw new HttpError(413,'Request is too large.');
    try { return typeof req.body === 'string' ? JSON.parse(req.body) : req.body; } catch {throw new HttpError(400,'Invalid JSON.');}
  }
  const chunks=[]; let size=0;
  for await (const chunk of req) {size+=chunk.length; if(size>150000) throw new HttpError(413,'Request is too large.'); chunks.push(chunk);}
  try {return JSON.parse(Buffer.concat(chunks).toString('utf8'));} catch {throw new HttpError(400,'Invalid JSON.');}
}
export function createHandler({env=process.env, fetcher=fetch}={}) {
  return async function handler(req, res) {
    securityHeaders(res);
    try {
      const cfg=config(env), url=new URL(req.url, cfg.origin);
      const routes={home:'/',login:'/auth/login',callback:'/auth/callback',logout:'/auth/logout',state:'/api/state',save:'/api/save',publish:'/api/publish'};
      const path=Object.hasOwn(routes,url.searchParams.get('route')) ? routes[url.searchParams.get('route')] : url.pathname;
      const token=unseal(readCookie(req,cfg,'session'),cfg,'session');
      if (path === '/auth/login' && req.method === 'GET') {
        const state=random(), verifier=random();
        res.setHeader('Set-Cookie',cookie(cfg,'oauth',seal({state,verifier},cfg,'oauth',600),600));
        const query=new URLSearchParams({client_id:cfg.clientId,redirect_uri:cfg.origin+'/auth/callback',state,code_challenge:challenge(verifier),code_challenge_method:'S256',allow_signup:'false',login:'OP22007'});
        if (cfg.authMode === 'oauth-app') query.set('scope','public_repo');
        return redirect(res,'https://github.com/login/oauth/authorize?'+query);
      }
      if (path === '/auth/callback' && req.method === 'GET') {
        const pending=unseal(readCookie(req,cfg,'oauth'),cfg,'oauth');
        res.setHeader('Set-Cookie',cookie(cfg,'oauth','',0));
        if (!pending || !equal(pending.state,url.searchParams.get('state')) || !url.searchParams.get('code')) throw new HttpError(403,'Sign-in expired or state did not match. Start sign-in again.');
        const response=await fetcher('https://github.com/login/oauth/access_token',{method:'POST',headers:{Accept:'application/json','Content-Type':'application/json'},body:JSON.stringify({client_id:cfg.clientId,client_secret:cfg.clientSecret,code:url.searchParams.get('code'),redirect_uri:cfg.origin+'/auth/callback',code_verifier:pending.verifier,...(cfg.authMode === 'github-app' ? {repository_id:cfg.repoId} : {})}),signal:AbortSignal.timeout(20000)});
        const auth=await response.json();
        if(!response.ok || typeof auth.access_token !== 'string' || auth.error) throw new HttpError(response.ok ? 403 : 502,authorizationError(auth.error));
        const gh=githubClient(auth.access_token,cfg,fetcher); const user=await requireOwner(gh,cfg);
        const lifetime=Math.min(14400, Number(auth.expires_in)||14400);
        res.setHeader('Set-Cookie',[cookie(cfg,'oauth','',0),cookie(cfg,'session',seal({token:auth.access_token,userId:user.id,csrf:random()},cfg,'session',lifetime),lifetime)]);
        return redirect(res,'/');
      }
      if (path === '/' && req.method === 'GET' && !token) {
        res.setHeader('Content-Type','text/html; charset=utf-8');
        return res.end('<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Owner sign-in · Sound Studio</title><link rel="stylesheet" href="/assets/admin.css"><main class="login"><span class="eyebrow">SOUND STUDIO / OWNER WORKSPACE</span><h1>Keep the evidence<br>as clear as the sound.</h1><p>A private workspace for model tests, compatibility and publication. Access is restricted to OP22007.</p><a class="primary" href="/auth/login">Sign in with GitHub ↗</a></main></html>');
      }
      if (!token || token.userId !== cfg.ownerId) throw new HttpError(401,'Sign in with the owner GitHub account.');
      const gh=githubClient(token.token,cfg,fetcher), user=await requireOwner(gh,cfg);
      if (req.method === 'POST') checkMutation(req,token,cfg);
      if (path === '/auth/logout' && req.method === 'POST') {
        res.setHeader('Set-Cookie',cookie(cfg,'session','',0));return json(res,200,{ok:true});
      }
      if (path === '/' && req.method === 'GET') {
        res.setHeader('Content-Type','text/html; charset=utf-8');return res.end(await readFile(new URL('../views/dashboard.html',import.meta.url),'utf8'));
      }
      if (path === '/api/state' && req.method === 'GET') {
        const snapshot=await readSnapshot(gh,cfg);
        const [registry,pages]=await Promise.all([readJSON(gh,cfg,'docs/compatibility/devices.json',snapshot.sha),readSnapshot(gh,cfg,'gh-pages')]);
        return json(res,200,{registry,baseSha:snapshot.sha,pagesSha:pages.sha,csrf:token.csrf,user:{login:user.login},branch:cfg.branch});
      }
      if (path === '/api/save' && req.method === 'POST') {
        const input=await body(req), snapshot=await readSnapshot(gh,cfg);
        if (input?.baseSha !== snapshot.sha) throw new HttpError(409,'The source changed. Reload before saving. Your current draft is still in this tab.');
        const original=await readJSON(gh,cfg,'docs/compatibility/devices.json',snapshot.sha);
        const registry=applyEdits(original,input,cfg.ownerId);
        const artwork=await readJSON(gh,cfg,'docs/compatibility/artwork.json',snapshot.sha);
        const rendered=generateCompatibility(registry,artwork);
        const files=new Map([['docs/compatibility/devices.json',JSON.stringify(registry,null,2)+'\n'],['docs/compatibility/README.md',rendered.get('docs/compatibility/README.md')],['docs/compatibility/admin-tests.md',testJournal(registry)]]);
        const sha=await commitFiles(gh,cfg,cfg.branch,snapshot,files,`Record owner compatibility review for ${input.modelId}`);
        return json(res,200,{registry,baseSha:sha,commitUrl:`https://github.com/${cfg.repo}/commit/${sha}`,published:false});
      }
      if (path === '/api/publish' && req.method === 'POST') {
        const input=await body(req), snapshot=await readSnapshot(gh,cfg);
        if (input?.baseSha !== snapshot.sha) throw new HttpError(409,'Source changed. Reload before publishing.');
        const pages=await readSnapshot(gh,cfg,'gh-pages');
        if(input.pagesSha !== pages.sha) throw new HttpError(409,'The public site changed. Reload before publishing.');
        const [registry,artwork]=await Promise.all([readJSON(gh,cfg,'docs/compatibility/devices.json',snapshot.sha),readJSON(gh,cfg,'docs/compatibility/artwork.json',snapshot.sha)]);
        const files=generateCompatibility(registry,artwork);files.delete('docs/compatibility/README.md');
        files.set('compatibility-version.json',JSON.stringify({sourceCommit:snapshot.sha,publishedAt:new Date().toISOString()})+'\n');
        const sha=await commitFiles(gh,cfg,'gh-pages',pages,files,`Publish compatibility from ${snapshot.sha.slice(0,7)}`);
        // A committed Pages branch and a completed deployment are different states.
        let deploymentRequested=true;
        try {await gh(`/repos/${cfg.repo}/pages/builds`,{},'POST');} catch {deploymentRequested=false;}
        return json(res,200,{pagesSha:sha,sourceSha:snapshot.sha,deploymentRequested,siteUrl:'https://op22007.github.io/Sony-Sound-Studio/devices/'});
      }
      throw new HttpError(404,'Not found.');
    } catch(error) {
      const status=error instanceof HttpError ? error.status : 500;
      // Never send upstream payloads, request data, cookies or tokens to the client or logs.
      return json(res,status,{error:status === 500 ? 'The operation failed. Reload and check GitHub history before retrying.' : error.message});
    }
  };
}
export default createHandler();
