'use strict';
const $ = id => document.getElementById(id);
const testKeys = ['testedOn','firmware','appVersion','osVersion','requested','readback','observed','steps','reference'];
let state, model, platform='windows', feature='connection', busy=false, formDirty=false;
const drafts = new Map();
const text = (tag,value,className) => {const el=document.createElement(tag);el.textContent=value;if(className)el.className=className;return el;};
function notify(message, error=false) { $('notice').textContent=message;$('notice').className=error?'notice error':'notice'; }
async function api(path, data) {
  const response=await fetch(path,{method:data?'POST':'GET',credentials:'same-origin',headers:data?{'Content-Type':'application/json','X-CSRF-Token':state.csrf}:{},...(data?{body:JSON.stringify(data)}:{})});
  const value=await response.json();
  if(!response.ok){if(response.status===401)notify('Your sign-in expired. Sign in again; copy any unsaved notes first.',true);throw new Error(value.error||'Request failed.');}
  return value;
}
function effective() {return {...state.registry.profiles?.[model.profile]?.platforms[platform],...model.platforms?.[platform]};}
function pending() {return drafts.size || (model && $('summary').value !== model.summary);}
function controls() {
  $('save').disabled=busy||!pending()||formDirty;
  $('publish').disabled=busy||!!pending()||formDirty;
  $('logout').disabled=busy;
  for(const el of document.querySelectorAll('input, select, textarea'))el.disabled=busy;
  for(const el of document.querySelectorAll('#test-form button, .models button, .platforms button, .features button, #drafts button'))el.disabled=busy;
}
function allowNavigation() {return !formDirty || confirm('This feature has edits that have not been added to review. Discard those edits?');}
function listModels() {
  const query=$('search').value.trim().toLowerCase();$('models').replaceChildren();
  const filtered=state.registry.devices.filter(m=>[m.name,...m.aliases,...m.productIds].join(' ').toLowerCase().includes(query));
  $('count').textContent=`${filtered.length} models`;
  for(const m of filtered){const button=text('button',m.name);button.type='button';button.setAttribute('aria-current',String(m.id===model?.id));button.addEventListener('click',()=>{
    if(busy||m.id===model?.id)return;
    if((pending()||formDirty)&&!confirm('Discard the current model’s unsaved review and choose another device?'))return;
    drafts.clear();model=m;feature='connection';renderModel();
  });$('models').append(button);}
}
function renderModel() {
  $('model-name').textContent=model.name;$('brand').textContent=model.brand+' / MODEL REVIEW';
  $('availability').textContent=({catalog:'Phone-app catalog · desktop support remains unverified',development:'Development source · not included in the released baseline',released:'Released preview profile'})[model.availability];
  $('summary').value=model.summary;$('review-date').textContent='Reviewed '+model.reviewedOn;
  $('artwork').hidden=!model.artwork;
  if(model.artwork)$('artwork').src='https://op22007.github.io/Sony-Sound-Studio/images/'+encodeURIComponent(model.artwork);
  listModels();renderFeatures();renderForm();renderDrafts();
}
function renderFeatures() {
  $('features').replaceChildren();
  for(const [key,label] of Object.entries(state.registry.features)){
    const cell=drafts.get(platform+'/'+key)||effective()[key];
    const button=text('button','');button.type='button';button.append(text('span',label),text('small',state.registry.statuses[cell.status]));button.dataset.status=cell.status;button.setAttribute('aria-pressed',String(feature===key));
    button.addEventListener('click',()=>{if(!allowNavigation())return;feature=key;renderFeatures();renderForm();});$('features').append(button);
  }
  for(const button of document.querySelectorAll('[data-platform]'))button.setAttribute('aria-pressed',String(button.dataset.platform===platform));
}
function renderForm() {
  const draft=drafts.get(platform+'/'+feature), cell=effective()[feature], current=draft||cell;
  $('feature-name').textContent=state.registry.features[feature];$('current-status').textContent=state.registry.statuses[cell.status];$('current-note').textContent=cell.note;
  $('current-evidence').replaceChildren();
  for(const id of cell.evidence){const record=state.registry.evidence[id];const details=document.createElement('details');details.append(text('summary',record.title),text('p',record.detail));$('current-evidence').append(details);}
  $('status').value=current.status;$('note').value=current.note;
  for(const key of testKeys)$(key).value=draft?.test?.[key]||'';
  $('testedOn').max=new Date().toISOString().slice(0,10);
  formDirty=false;validation();controls();
}
function validation() {
  const confirmed=$('status').value==='confirmed';
  for(const key of testKeys)$(key).required=confirmed&&key!=='reference';
}
function renderDrafts() {
  $('draft-count').textContent=`${drafts.size} feature ${drafts.size===1?'change':'changes'}`;$('drafts').replaceChildren();
  for(const [key,draft] of drafts){const li=document.createElement('li');li.append(text('span',`${draft.platform} / ${state.registry.features[draft.feature]} → ${state.registry.statuses[draft.status]}`));const remove=text('button','Remove','quiet');remove.type='button';remove.addEventListener('click',()=>{drafts.delete(key);renderDrafts();renderFeatures();if(key===platform+'/'+feature)renderForm();});li.append(remove);$('drafts').append(li);}
  if(!drafts.size)$('drafts').append(text('li','Add feature results above, or edit the model summary.','muted'));
  controls();
}
$('search').addEventListener('input',()=>{if(state)listModels();});
$('summary').addEventListener('input',controls);
for(const button of document.querySelectorAll('[data-platform]'))button.addEventListener('click',()=>{if(!allowNavigation())return;platform=button.dataset.platform;renderFeatures();renderForm();});
$('test-form').addEventListener('input',()=>{formDirty=true;validation();controls();});
$('status').addEventListener('change',()=>{formDirty=true;validation();controls();});
$('test-form').addEventListener('submit',event=>{
  event.preventDefault();drafts.set(platform+'/'+feature,{platform,feature,status:$('status').value,note:$('note').value,test:Object.fromEntries(testKeys.map(key=>[key,$(key).value]))});formDirty=false;renderDrafts();renderFeatures();notify('Result added to this model’s review. Save when the review is ready.');
});
$('save').addEventListener('click',async()=>{
  busy=true;controls();notify('Saving the review to GitHub…');
  try {const result=await api('/api/save',{baseSha:state.baseSha,modelId:model.id,summary:$('summary').value,changes:[...drafts.values()]});state.registry=result.registry;state.baseSha=result.baseSha;model=state.registry.devices.find(m=>m.id===model.id);drafts.clear();renderModel();notify('Saved to GitHub. The public site has not changed yet.');$('publication').textContent='Saved source commit '+result.baseSha.slice(0,7)+'. Ready to publish.';}
  catch(error){notify(error.message,true);}finally{busy=false;controls();}
});
$('publish').addEventListener('click',async()=>{
  busy=true;controls();notify('Publishing the saved compatibility pages…');
  try{const result=await api('/api/publish',{baseSha:state.baseSha,pagesSha:state.pagesSha});state.pagesSha=result.pagesSha;notify(result.deploymentRequested?'Pages commit saved and deployment requested. GitHub is deploying it.':'Pages commit saved, but the deployment request failed. Check GitHub Pages before retrying.',!result.deploymentRequested);$('publication').textContent=`Pages commit ${result.pagesSha.slice(0,7)} · source ${result.sourceSha.slice(0,7)}. Deployment completion has not been verified.`;}
  catch(error){notify(error.message,true);}finally{busy=false;controls();}
});
$('logout').addEventListener('click',async()=>{if((pending()||formDirty)&&!confirm('Sign out and discard unsaved edits?'))return;try{await api('/auth/logout',{});location.assign('/');}catch(error){notify(error.message,true);}});
window.addEventListener('beforeunload',event=>{if(pending()||formDirty){event.preventDefault();event.returnValue='';}});
(async()=>{try{state=await api('/api/state');$('account').textContent=state.user.login+' · owner';for(const [value,label] of Object.entries(state.registry.statuses)){const option=text('option',label);option.value=value;$('status').append(option);}model=state.registry.devices[0];$('content').hidden=false;renderModel();notify('Owner access verified. Saved reviews and public publication are separate steps.');}catch(error){notify(error.message,true);}})();
