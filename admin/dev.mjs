import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import handler from './server/handler.mjs';
import { securityHeaders } from './server/security.mjs';

const assets = new Map([['/assets/admin.js',['admin.js','text/javascript']],['/assets/admin.css',['admin.css','text/css']]]);
createServer(async(req,res)=>{
  const asset=assets.get(new URL(req.url,'http://localhost').pathname);
  if(asset && req.method==='GET'){
    securityHeaders(res);res.setHeader('Content-Type',asset[1]);res.end(await readFile(new URL('./public/assets/'+asset[0],import.meta.url)));return;
  }
  await handler(req,res);
}).listen(4330,'127.0.0.1',()=>console.log('Owner admin: http://127.0.0.1:4330 — real GitHub sign-in is required.'));
