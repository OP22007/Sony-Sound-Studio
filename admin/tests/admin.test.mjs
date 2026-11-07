import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { Readable } from 'node:stream';
import { config, seal, unseal, cookie, random, challenge } from '../server/security.mjs';
import { applyEdits, effectiveModel, testJournal } from '../server/edits.mjs';
import { generateCompatibility } from '../lib/compatibility.mjs';
import { createHandler } from '../server/handler.mjs';

const baseline=JSON.parse(await readFile(new URL('../../docs/compatibility/devices.json',import.meta.url)));
const artwork=JSON.parse(await readFile(new URL('../../docs/compatibility/artwork.json',import.meta.url)));
const env={ADMIN_ORIGIN:'https://studio-admin.example',GITHUB_CLIENT_ID:'test-client',GITHUB_CLIENT_SECRET:'test-secret',SESSION_SECRET:'ae'.repeat(32)};
const cfg=config(env), session={token:'test-access-token',userId:cfg.ownerId,csrf:random()};
const input=()=>({baseSha:'source-head',modelId:'wh-1000xm6',summary:baseline.devices.find(m=>m.id==='wh-1000xm6').summary,changes:[{platform:'windows',feature:'noise',status:'confirmed',note:'Three modes and hardware-button tracking verified.',test:{firmware:'3.1.5',appVersion:'0.1.3',testedOn:'2026-01-01',osVersion:'Windows 11',requested:'ANC, Ambient, Off',readback:'Matched each requested mode',observed:'Audible changes matched all three modes',steps:'Switch three modes, wait, then use the headphone button.'}}]});
function mockGitHub({ownerId=cfg.ownerId,refConflict=false,failPagesBuild=false,failPatch=false}={}) {
  const calls=[];
  const fetcher=async(url,options={})=>{
    const path=new URL(url).pathname, method=options.method||'GET', data=options.body?JSON.parse(options.body):undefined;
    calls.push({url,path,method,data});
    let result;
    if(url==='https://github.com/login/oauth/access_token')result={access_token:'test-access-token'};
    else if(path==='/user')result={id:ownerId,login:'OP22007'};
    else if(path.includes('/git/ref/heads/'))result={object:{sha:path.endsWith('gh-pages')?'pages-head':refConflict?'changed-head':'source-head'}};
    else if(path.includes('/git/commits/')&&method==='GET')result={tree:{sha:'base-tree'}};
    else if(path.includes('/contents/docs/compatibility/')){const value=path.endsWith('artwork.json')?artwork:baseline;result={encoding:'base64',content:Buffer.from(JSON.stringify(value)).toString('base64')};}
    else if(path.endsWith('/git/trees'))result={sha:'new-tree'};
    else if(path.endsWith('/git/commits'))result={sha:'new-commit'};
    else if(path.includes('/git/refs/')&&method==='PATCH'){if(failPatch)return new Response('{}',{status:422});result={object:{sha:data.sha}};}
    else if(path.endsWith('/pages/builds')){if(failPagesBuild)return new Response('{}',{status:403});result={status:'queued'};}
    else throw new Error('Unexpected test request '+method+' '+path);
    return new Response(JSON.stringify(result),{headers:{'Content-Type':'application/json'}});
  };
  return {fetcher,calls};
}
async function request(handler,path,{method='GET',signedIn=false,payload,headers={},cookieValue}={}) {
  const body=payload===undefined?'':JSON.stringify(payload);
  const req=Readable.from(body?[Buffer.from(body)]:[]);req.url=path;req.method=method;
  req.headers={...(signedIn?{cookie:cookie(cfg,'session',seal(session,cfg,'session')).split(';')[0]}:{}),...(method==='POST'?{origin:cfg.origin,'content-type':'application/json','x-csrf-token':session.csrf}:{}),...(cookieValue?{cookie:cookieValue}:{}),...headers};
  const res={statusCode:200,headers:{},setHeader(key,value){this.headers[key.toLowerCase()]=value;},end(value=''){this.body=String(value);}};
  await handler(req,res);res.json=()=>JSON.parse(res.body);return res;
}

test('configuration fails closed and pins owner ID, repository and working branch',()=>{
  assert.throws(()=>config({}),/required/);assert.throws(()=>config({...env,SESSION_SECRET:'short'}),/32 random bytes/);
  assert.throws(()=>config({...env,ADMIN_ORIGIN:'http://unsafe.example'}),/HTTPS/);
  assert.throws(()=>config({...env,SOURCE_BRANCH:'main'}),/codex/);
  assert.throws(()=>config({...env,GITHUB_AUTH_MODE:'unknown'}),/GITHUB_AUTH_MODE/);
  assert.equal(config({...env,ADMIN_GITHUB_USER_ID:'999'}).ownerId,140067318);
});
test('sealed sessions reject tampering, expiry, wrong purpose and different origin',()=>{
  const value=seal(session,cfg,'session');assert.equal(unseal(value,cfg,'session').token,session.token);
  const decoded=Buffer.from(value,'base64url');decoded[35]^=1;assert.equal(unseal(decoded.toString('base64url'),cfg,'session'),null);
  assert.equal(unseal(value,cfg,'oauth'),null);assert.equal(unseal(seal(session,cfg,'session',-1),cfg,'session'),null);
  assert.equal(unseal(value,{...cfg,origin:'https://other.example'},'session'),null);
  assert.match(cookie(cfg,'session',value),/__Host-.*HttpOnly; SameSite=Lax;.*Secure/);
});
test('anonymous visitors cannot load the editor or private APIs',async()=>{
  const mock=mockGitHub(), handler=createHandler({env,fetcher:mock.fetcher});
  const landing=await request(handler,'/');assert.match(landing.body,/Sign in with GitHub/);assert.doesNotMatch(landing.body,/test-form/);
  for(const [path,method] of [['/api/state','GET'],['/api/save','POST'],['/api/publish','POST']])assert.equal((await request(handler,path,{method,payload:{}})).statusCode,401);
  assert.equal(mock.calls.length,0);
  assert.match(landing.headers['content-security-policy'],/frame-ancestors 'none'/);assert.equal(landing.headers['cache-control'],'no-store');
});
test('another account is denied even when it has the same login label',async()=>{
  const mock=mockGitHub({ownerId:999}),handler=createHandler({env,fetcher:mock.fetcher});
  for(const path of ['/','/api/state','/api/save','/api/publish'])assert.equal((await request(handler,path,{signedIn:true,method:path.includes('save')||path.includes('publish')?'POST':'GET',payload:{}})).statusCode,403);
  assert.ok(mock.calls.every(call=>call.path==='/user'));
});
test('missing setup never serves the private editor',async()=>{
  const response=await request(createHandler({env:{}}),'/');assert.equal(response.statusCode,503);assert.doesNotMatch(response.body,/test-form/);
});
test('OAuth uses state and S256 PKCE, rejects state mismatch before token exchange',async()=>{
  const mock=mockGitHub(),handler=createHandler({env,fetcher:mock.fetcher});
  const start=await request(handler,'/auth/login');const url=new URL(start.headers.location);
  assert.equal(url.searchParams.get('code_challenge_method'),'S256');assert.equal(url.searchParams.get('redirect_uri'),cfg.origin+'/auth/callback');
  assert.equal(url.searchParams.get('scope'),'public_repo');
  const cookieValue=start.headers['set-cookie'].split(';')[0],pending=unseal(cookieValue.split('=')[1],cfg,'oauth');
  assert.equal(url.searchParams.get('code_challenge'),challenge(pending.verifier));
  const bad=await request(handler,'/auth/callback?code=test-code&state=wrong',{cookieValue});assert.equal(bad.statusCode,403);assert.equal(mock.calls.length,0);
  const good=await request(handler,`/auth/callback?code=test-code&state=${pending.state}`,{cookieValue});assert.equal(good.statusCode,303);
  assert.match(good.headers['set-cookie'][1],/__Host-studio_session=/);assert.doesNotMatch(good.headers['set-cookie'][1],/test-access-token/);
  assert.equal(mock.calls[0].data.code_verifier,pending.verifier);
  assert.equal(mock.calls[0].data.repository_id,undefined);
});
test('GitHub App sign-in omits OAuth scopes and restricts the token to the fixed repository',async()=>{
  const appEnv={...env,GITHUB_AUTH_MODE:'github-app'},mock=mockGitHub(),handler=createHandler({env:appEnv,fetcher:mock.fetcher});
  const start=await request(handler,'/auth/login'),url=new URL(start.headers.location);
  assert.equal(url.searchParams.has('scope'),false);
  assert.equal(url.searchParams.get('code_challenge_method'),'S256');
  const cookieValue=start.headers['set-cookie'].split(';')[0],pending=unseal(cookieValue.split('=')[1],config(appEnv),'oauth');
  const result=await request(handler,`/auth/callback?code=test-code&state=${pending.state}`,{cookieValue});
  assert.equal(result.statusCode,303);
  assert.equal(mock.calls[0].data.repository_id,1441020213);
  assert.equal(mock.calls[0].data.code_verifier,pending.verifier);
});
test('OAuth callback rejects a non-owner before issuing a session',async()=>{
  const mock=mockGitHub({ownerId:999}),handler=createHandler({env,fetcher:mock.fetcher});const pending={state:random(),verifier:random()};
  const result=await request(handler,`/auth/callback?code=x&state=${pending.state}`,{cookieValue:cookie(cfg,'oauth',seal(pending,cfg,'oauth')).split(';')[0]});
  assert.equal(result.statusCode,403);assert.equal(typeof result.headers['set-cookie'],'string');
});
test('writes reject wrong origins, missing CSRF, non-JSON and oversized input',async()=>{
  const mock=mockGitHub(),handler=createHandler({env,fetcher:mock.fetcher});
  for(const headers of [{origin:'https://attacker.example'},{'x-csrf-token':''},{'content-type':'text/plain'}]){
    const result=await request(handler,'/api/save',{method:'POST',signedIn:true,payload:input(),headers});assert.ok([403,415].includes(result.statusCode));
  }
  const result=await request(handler,'/api/save',{method:'POST',signedIn:true,payload:{...input(),padding:'x'.repeat(150001)}});assert.equal(result.statusCode,413);
  assert.ok(mock.calls.every(c=>c.path==='/user'));
});
test('owner can load private dashboard and state without receiving a GitHub token',async()=>{
  const mock=mockGitHub(),handler=createHandler({env,fetcher:mock.fetcher});
  assert.match((await request(handler,'/',{signedIn:true})).body,/test-form/);
  const response=await request(handler,'/api/state',{signedIn:true});assert.equal(response.statusCode,200);assert.equal(response.json().registry.devices.length,92);assert.equal(response.json().csrf,session.csrf);assert.doesNotMatch(response.body,/test-access-token|test-secret/);
});
test('confirmation requires complete hardware evidence and a real test date',()=>{
  for(const key of ['firmware','appVersion','testedOn','osVersion','requested','readback','observed','steps']){
    const data=input();delete data.changes[0].test[key];assert.throws(()=>applyEdits(baseline,data,cfg.ownerId),/requires/);
  }
  const data=input();data.changes[0].test.testedOn='2026-02-31';assert.throws(()=>applyEdits(baseline,data,cfg.ownerId),/real test date/);
  data.changes[0].test.testedOn='2026-99-99';assert.throws(()=>applyEdits(baseline,data,cfg.ownerId),/real test date/);
});
test('edits affect one model and feature, retain existing confirmations and generate linked evidence',()=>{
  const original=JSON.stringify(baseline),data=input();data.changes[0].feature='eq';data.changes[0].test.firmware='different-firmware';
  const updated=applyEdits(baseline,data,cfg.ownerId);assert.equal(JSON.stringify(baseline),original);
  const model=updated.devices.find(m=>m.id===data.modelId),id=model.platforms.windows.eq.evidence[0];
  assert.equal(updated.evidence[id].test.firmware,'different-firmware');assert.deepEqual(updated.devices.find(m=>m.id==='wh-1000xm5'),baseline.devices.find(m=>m.id==='wh-1000xm5'));
  const rendered=generateCompatibility(updated,artwork);assert.match(rendered.get(`devices/${model.id}/index.html`),/different-firmware/);assert.match(testJournal(updated),new RegExp(id));
  assert.equal(rendered.size,94);
});
test('changing a catalog model does not mutate shared profile defaults',()=>{
  const data=input();data.modelId='wh-1000xm4';data.summary='Hardware test pending.';data.changes[0].status='issue';
  const updated=applyEdits(baseline,data,cfg.ownerId);assert.deepEqual(updated.profiles,baseline.profiles);
  assert.equal(effectiveModel(updated,updated.devices.find(m=>m.id===data.modelId)).platforms.windows.noise.status,'issue');
});
test('unknown features, statuses, duplicates and dangerous links are refused',()=>{
  for(const [key,value] of [['feature','__proto__'],['platform','android'],['status','invented']]){const data=input();data.changes[0][key]=value;assert.throws(()=>applyEdits(baseline,data,cfg.ownerId),/Unknown/);}
  const duplicate=input();duplicate.changes.push(duplicate.changes[0]);assert.throws(()=>applyEdits(baseline,duplicate,cfg.ownerId),/Duplicate/);
  const link=input();link.changes[0].test.reference='javascript:alert(1)';assert.throws(()=>applyEdits(baseline,link,cfg.ownerId),/HTTPS/);
});
test('public HTML and test journal escape entered markup',()=>{
  const data=input();data.changes[0].note='<script>alert(1)</script>\nA | B';data.summary='<img src=x onerror=alert(1)>';
  const updated=applyEdits(baseline,data,cfg.ownerId),files=generateCompatibility(updated,artwork),html=files.get('devices/wh-1000xm6/index.html');
  assert.doesNotMatch(html,/<script>alert/);assert.match(html,/&lt;script&gt;/);assert.doesNotMatch(testJournal(updated),/<script>/);
});
test('stale saves and stale publications fail before writes',async()=>{
  for(const path of ['/api/save','/api/publish']){
    const mock=mockGitHub(),handler=createHandler({env,fetcher:mock.fetcher});const response=await request(handler,path,{method:'POST',signedIn:true,payload:{...input(),baseSha:'stale',pagesSha:'stale'}});
    assert.equal(response.statusCode,409);assert.ok(mock.calls.every(c=>c.method==='GET'));
  }
});
test('save atomically commits registry, GitHub table and journal without publishing',async()=>{
  const mock=mockGitHub(),handler=createHandler({env,fetcher:mock.fetcher});const response=await request(handler,'/api/save',{method:'POST',signedIn:true,payload:input()});assert.equal(response.statusCode,200);assert.equal(response.json().published,false);
  const tree=mock.calls.find(c=>c.path.endsWith('/git/trees'));assert.deepEqual(tree.data.tree.map(f=>f.path).sort(),['docs/compatibility/README.md','docs/compatibility/admin-tests.md','docs/compatibility/devices.json']);
  const patch=mock.calls.find(c=>c.method==='PATCH');assert.equal(patch.data.force,false);assert.match(patch.path,/heads\/codex\//);assert.ok(!mock.calls.some(c=>c.path.includes('pages/builds')));
});
test('publication preserves assets and reports deployment request failure separately',async()=>{
  const mock=mockGitHub({failPagesBuild:true}),handler=createHandler({env,fetcher:mock.fetcher});const response=await request(handler,'/api/publish',{method:'POST',signedIn:true,payload:{baseSha:'source-head',pagesSha:'pages-head'}});
  assert.equal(response.statusCode,200);assert.equal(response.json().deploymentRequested,false);
  const tree=mock.calls.find(c=>c.path.endsWith('/git/trees'));assert.equal(tree.data.base_tree,'base-tree');assert.ok(tree.data.tree.every(f=>f.path.startsWith('devices/')||f.path==='compatibility-version.json'));assert.ok(tree.data.tree.every(f=>!Object.hasOwn(f,'sha')));
});
test('non-fast-forward rejection does not report a successful save',async()=>{
  const mock=mockGitHub({failPatch:true}),handler=createHandler({env,fetcher:mock.fetcher});const response=await request(handler,'/api/save',{method:'POST',signedIn:true,payload:input()});assert.equal(response.statusCode,409);assert.ok(!response.json().baseSha);
});
test('logout requires CSRF and clears the sealed browser session',async()=>{
  const handler=createHandler({env,fetcher:mockGitHub().fetcher});const response=await request(handler,'/auth/logout',{method:'POST',signedIn:true,payload:{}});assert.equal(response.statusCode,200);assert.match(response.headers['set-cookie'],/Max-Age=0/);
});
