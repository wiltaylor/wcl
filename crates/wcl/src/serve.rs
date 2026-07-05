use std::future::IntoFuture;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use tempfile::TempDir;
use tokio::sync::mpsc::UnboundedReceiver;
use wcl_wdoc::{BuildOptions, RebuildOutcome, build_incremental, build_with_options, comments};

/// How long the watch loop waits for the event stream to go quiet
/// before rebuilding — one editor save fires several notify events,
/// which should coalesce into a single build.
const QUIET_WINDOW: Duration = Duration::from_millis(150);

/// How long a live-reload long-poll request parks before answering
/// with the unchanged generation. Short enough that intermediaries
/// don't kill the connection; the client just re-polls.
const POLL_TIMEOUT: Duration = Duration::from_secs(30);

/// The address `serve` binds to when neither `--addr` nor `auto` shifts
/// it elsewhere. `auto` scans upward from this port.
pub(crate) const DEFAULT_BIND: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 8080);

/// How the dev server chooses its bind address.
///
/// `--addr auto` picks the first free port near [`DEFAULT_BIND`]; any other
/// value is parsed as an explicit `SocketAddr` and bound as-is (hard error if
/// the port is busy).
#[derive(Debug, Clone, Copy)]
pub(crate) enum BindSpec {
    /// Scan upward from [`DEFAULT_BIND`] for a free port.
    Auto,
    /// Bind exactly this address.
    Fixed(SocketAddr),
}

impl std::str::FromStr for BindSpec {
    type Err = std::net::AddrParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case("auto") {
            Ok(BindSpec::Auto)
        } else {
            s.parse().map(BindSpec::Fixed)
        }
    }
}

/// Bind a listener by scanning a fixed window of ports upward from `base`,
/// returning the first that's free. Keeping the successfully bound listener
/// avoids a check-then-bind race.
async fn bind_auto(base: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    const RANGE: u16 = 100; // scan base..base+100
    let mut last_err = None;
    for offset in 0..RANGE {
        let Some(port) = base.port().checked_add(offset) else {
            break;
        };
        let cand = SocketAddr::new(base.ip(), port);
        match tokio::net::TcpListener::bind(cand).await {
            Ok(l) => return Ok(l),
            Err(e) if e.kind() == ErrorKind::AddrInUse => last_err = Some(e),
            Err(e) => return Err(e),
        }
    }
    Err(last_err.unwrap_or_else(|| {
        std::io::Error::new(ErrorKind::AddrInUse, "no free port found near default")
    }))
}

/// Injected into every served HTML page (and the error/404 pages):
/// long-polls `/__wdoc_reload` and reloads when the build generation
/// changes, so the browser tracks rebuilds — including the flip
/// between a working site and the build-failure page.
const RELOAD_SCRIPT: &str = "<script>(async()=>{const u='/__wdoc_reload';let g=null;\
for(;;){try{const r=await fetch(g===null?u:u+'?gen='+encodeURIComponent(g));\
const t=(await r.text()).trim();if(g!==null&&t!==g){location.reload();return}g=t}\
catch(e){await new Promise(r=>setTimeout(r,1000))}}})();</script>";

/// Appended to served HTML when comment mode is on: loads the comment client.
const COMMENT_SCRIPT_TAG: &str = "<script src=\"/__wdoc_comment.js\"></script>";

/// The comment-mode client, served at `/__wdoc_comment.js`. A toolbar toggles
/// **select mode**: while it is on, hovering highlights a `[data-wcl-block]` and
/// clicking it (a whole block — works for tables and diagrams, no text
/// selection needed) opens the comment form. A second button comments on the
/// page as a whole. It computes a locator for generated blocks, POSTs to
/// `/__wdoc_comment`, and re-shows existing comments inline as pins.
const COMMENT_CLIENT_JS: &str = r#"(()=>{
const CSS = `
body.wcl-picking{cursor:crosshair}
body.wcl-picking [data-wcl-block].wcl-hot{outline:2px solid #4c8bf5;outline-offset:2px;
 background:rgba(76,139,245,.10)}
[data-wcl-comment-id]{position:relative;outline:2px dashed #e0a000;outline-offset:1px}
.wcl-pin{position:absolute;top:-10px;right:-10px;width:20px;height:20px;border-radius:50%;
 background:#e0a000;color:#1c1c1c;font:bold 12px system-ui;display:flex;align-items:center;
 justify-content:center;cursor:pointer;z-index:99998}
.wcl-pop{position:absolute;z-index:100000;background:#1c1c1c;color:#eee;border:1px solid #444;
 border-radius:8px;padding:10px;width:300px;font:13px system-ui;box-shadow:0 8px 30px rgba(0,0,0,.5)}
.wcl-pop textarea{width:100%;box-sizing:border-box;min-height:64px;background:#111;color:#eee;
 border:1px solid #444;border-radius:6px;padding:6px;font:13px system-ui;resize:vertical}
.wcl-pop .wcl-q-prev{font-style:italic;opacity:.8;margin:0 0 6px;border-left:3px solid #e0a000;padding-left:6px}
.wcl-pop .row{display:flex;gap:6px;justify-content:flex-end;margin-top:8px}
.wcl-pop button{background:#4c8bf5;color:#fff;border:0;border-radius:6px;padding:5px 12px;cursor:pointer;font:13px system-ui}
.wcl-pop button.ghost{background:#333}
.wcl-err{color:#f88;margin-top:8px;font-size:12px;white-space:pre-wrap;max-height:8em;overflow:auto}
.wcl-bar{position:fixed;bottom:18px;right:18px;z-index:99999;display:flex;flex-direction:column;gap:8px;align-items:flex-end}
.wcl-actions{display:flex;flex-direction:column;gap:8px;align-items:flex-end;
 opacity:0;transform:translateY(8px);pointer-events:none;transition:opacity .15s ease,transform .15s ease}
.wcl-bar.wcl-open .wcl-actions{opacity:1;transform:none;pointer-events:auto}
.wcl-bar button{background:#4c8bf5;color:#fff;border:0;border-radius:20px;padding:9px 16px;
 font:600 13px system-ui;cursor:pointer;box-shadow:0 6px 20px rgba(0,0,0,.4)}
.wcl-bar button.on{background:#e0a000;color:#1c1c1c}
.wcl-bar button.wcl-count{background:#2f6f4f}
.wcl-bar button.wcl-toggle{position:relative;width:48px;height:48px;padding:0;border-radius:50%;font-size:20px}
.wcl-bar button.wcl-toggle .wcl-badge{position:absolute;top:-4px;right:-4px;min-width:18px;height:18px;
 box-sizing:border-box;padding:0 4px;border-radius:9px;background:#e0a000;color:#1c1c1c;
 font:bold 11px system-ui;display:none;align-items:center;justify-content:center}
.wcl-bar:not(.wcl-open) button.wcl-toggle .wcl-badge.on{display:flex}
.wcl-hint{position:fixed;top:0;left:0;right:0;z-index:99999;background:#4c8bf5;color:#fff;
 text-align:center;padding:7px;font:600 13px system-ui}
.wcl-rebuild{position:fixed;top:0;left:0;right:0;z-index:100002;padding:8px;text-align:center;
 font:600 13px system-ui;color:#fff;display:flex;align-items:center;justify-content:center;gap:8px}
.wcl-rb-running{background:#4c8bf5}
.wcl-rb-done{background:#2f6f4f}
.wcl-rb-error{background:#b91c1c}
.wcl-rb-spin{width:14px;height:14px;border:2px solid rgba(255,255,255,.4);border-top-color:#fff;
 border-radius:50%;display:inline-block;animation:wcl-spin .7s linear infinite}
@keyframes wcl-spin{to{transform:rotate(360deg)}}
[data-wcl-block].wcl-flash{outline:3px solid #e0a000!important;outline-offset:2px!important}
.wcl-modal{position:fixed;inset:0;z-index:100001;background:rgba(0,0,0,.5);display:flex;
 align-items:center;justify-content:center}
.wcl-modal-box{background:#1c1c1c;color:#eee;border:1px solid #444;border-radius:10px;
 width:min(560px,92vw);max-height:80vh;overflow:auto;font:13px system-ui;box-shadow:0 12px 50px rgba(0,0,0,.6)}
.wcl-modal-h{display:flex;justify-content:space-between;align-items:center;gap:12px;padding:12px 14px;
 border-bottom:1px solid #333;font-weight:600;position:sticky;top:0;background:#1c1c1c}
.wcl-modal-h button{background:#333;color:#eee;border:0;border-radius:6px;padding:4px 10px;cursor:pointer}
.wcl-c{padding:12px 14px;border-bottom:1px solid #2a2a2a}
.wcl-c .meta{opacity:.7;font-size:12px;margin-bottom:4px}
.wcl-c .q{font-style:italic;opacity:.85;border-left:3px solid #e0a000;padding-left:6px;margin:4px 0}
.wcl-c .acts{margin-top:8px;display:flex;gap:8px}
.wcl-c button{background:#4c8bf5;color:#fff;border:0;border-radius:6px;padding:4px 10px;cursor:pointer;font:12px system-ui}
.wcl-c button.ghost{background:#333}
.wcl-c textarea{width:100%;box-sizing:border-box;min-height:56px;margin-top:4px;background:#111;color:#eee;
 border:1px solid #444;border-radius:6px;padding:6px;font:13px system-ui;resize:vertical}
.wcl-empty{padding:24px;text-align:center;opacity:.7}
.wcl-review-bar{position:fixed;top:0;left:0;right:0;z-index:100001;background:#7c3aed;color:#fff;
 padding:9px 14px;font:600 13px system-ui;display:flex;align-items:center;justify-content:center;gap:12px;
 box-shadow:0 2px 12px rgba(0,0,0,.3)}
.wcl-review-bar button{background:#fff;color:#7c3aed;border:0;border-radius:6px;padding:5px 12px;
 cursor:pointer;font:600 13px system-ui}
.wcl-review-bar button.ghost{background:rgba(255,255,255,.22);color:#fff}
.wcl-review-bar button:disabled{opacity:.6;cursor:default}
`;
const st=document.createElement('style');st.textContent=CSS;document.head.appendChild(st);

const pageEl=document.querySelector('[data-wcl-page-file]');
function esc(s){return (s||'').replace(/[&<>]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;'}[c]));}
function chrome(t){return t.closest('.wcl-pop')||t.closest('.wcl-bar')||t.closest('.wcl-hint')||t.closest('.wcl-pin');}

// Block tree: a block's children are the nearest [data-wcl-block] descendants
// of `node` not separated from it by another block. `locOf` / `elByLoc` are an
// invertible pair over this tree — locOf emits the child-index path from the
// page root to `el`, elByLoc walks that path back to the element.
function blockChildren(node){
 return [...node.querySelectorAll('[data-wcl-block]')].filter(b=>{
  let p=b.parentElement;
  while(p&&p!==node){if(p.hasAttribute('data-wcl-block'))return false;p=p.parentElement;}
  return p===node;
 });
}
function locOf(el){
 const path=[];let cur=el;
 while(cur&&cur!==pageEl){
  let p=cur.parentElement,pb=null;
  while(p&&p!==pageEl){if(p.hasAttribute('data-wcl-block')){pb=p;break;}p=p.parentElement;}
  path.unshift(blockChildren(pb||pageEl).indexOf(cur));
  cur=pb;
 }
 return path.join('/');
}
function elByLoc(loc){
 if(loc===''||loc==null||!pageEl)return null;
 let node=pageEl;
 for(const part of String(loc).split('/')){
  node=blockChildren(node)[+part];
  if(!node)return null;
 }
 return node;
}
function descOf(el){
 const kind=el.getAttribute('data-wcl-kind')||'block';
 const txt=(el.textContent||'').trim().replace(/\s+/g,' ').slice(0,60);
 return txt?`${kind} — "${txt}"`:kind;
}

let pop=null;
function closePop(){if(pop){pop.remove();pop=null;}}
// Show a server error inside the popup instead of swallowing it.
function showErr(p,txt){
 let e=p.querySelector('.wcl-err');
 if(!e){e=document.createElement('div');e.className='wcl-err';p.appendChild(e);}
 let m=txt;try{m=JSON.parse(txt).error||txt;}catch(_){}
 e.textContent='⚠ '+m;
}
// POST JSON; on a non-2xx response surface the error in `p` and return false.
async function post(payload,p,url){
 const u=url||'/__wdoc_comment';
 let res;
 try{res=await fetch(u,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(payload)});}
 catch(err){showErr(p,String(err));return false;}
 if(!res.ok){showErr(p,await res.text());return false;}
 return true;
}

function openForm(el,onPage){
 closePop();
 const quote=(!onPage && window.getSelection&&String(window.getSelection()).trim())||'';
 const r=el.getBoundingClientRect();
 pop=document.createElement('div');pop.className='wcl-pop';
 pop.style.top=(window.scrollY+Math.max(8,r.top)+24)+'px';
 pop.style.left=(window.scrollX+Math.max(8,Math.min(r.left,window.innerWidth-320)))+'px';
 pop.innerHTML=(quote?`<p class="wcl-q-prev">${esc(quote)}</p>`:'')+
   `<textarea placeholder="Leave a comment…"></textarea>
    <div class="row"><button class="ghost" data-x>Cancel</button><button data-ok>Comment</button></div>`;
 document.body.appendChild(pop);
 const ta=pop.querySelector('textarea');ta.focus();
 pop.querySelector('[data-x]').onclick=closePop;
 pop.querySelector('[data-ok]').onclick=async()=>{
  const body=ta.value.trim();if(!body||!pageEl)return;
  // The page key (name + source file) routes the comment to the owning
  // comments.wcl sidecar; a block comment also carries its locator + target.
  const payload={
   body,quote:quote||null,
   page:pageEl.getAttribute('data-wcl-page-name'),
   page_file:pageEl.getAttribute('data-wcl-page-file'),
  };
  if(!onPage){payload.loc=locOf(el);payload.target=descOf(el);}
  // Write the sidecar, then re-fetch + re-pin client-side — no rebuild.
  if(await post(payload,pop)){closePop();await refresh();}
 };
}

// Select mode: explicit toggle so normal clicks still navigate. While on,
// hovering highlights the block under the cursor and a click picks it.
let picking=false,hot=null,hint=null;
function setPick(on){
 picking=on;
 document.body.classList.toggle('wcl-picking',on);
 selBtn.classList.toggle('on',on);
 selBtn.textContent=on?'✕ Cancel':'🎯 Comment on a block';
 if(hot){hot.classList.remove('wcl-hot');hot=null;}
 if(on && !hint){hint=document.createElement('div');hint.className='wcl-hint';
  hint.textContent='Click a block to comment on it — Esc to cancel';document.body.appendChild(hint);}
 if(!on && hint){hint.remove();hint=null;}
}
document.addEventListener('mousemove',e=>{
 if(!picking)return;
 const el=chrome(e.target)?null:(e.target.closest&&e.target.closest('[data-wcl-block]'));
 if(el!==hot){if(hot)hot.classList.remove('wcl-hot');hot=el;if(hot)hot.classList.add('wcl-hot');}
});
document.addEventListener('click',e=>{
 if(!picking||chrome(e.target))return;
 const el=e.target.closest&&e.target.closest('[data-wcl-block]');
 if(el){e.preventDefault();e.stopPropagation();setPick(false);openForm(el,false);}
},true);
document.addEventListener('keydown',e=>{if(e.key==='Escape'){if(picking)setPick(false);else{closePop();closeModal();}}});

// Existing block comments: resolve each stored locator to its element and add
// a pin + outline (the dashed-outline CSS keys off data-wcl-comment-id, which
// we now set client-side). Whole-page comments (no loc) show only in the modal.
function placePins(){
 for(const el of document.querySelectorAll('[data-wcl-comment-id]')){
  el.removeAttribute('data-wcl-comment-id');el.removeAttribute('data-wcl-comment');
  el.querySelectorAll(':scope > .wcl-pin').forEach(p=>p.remove());
 }
 for(const c of pageComments()){
  if(!c.loc)continue;
  const el=elByLoc(c.loc);if(!el)continue;
  el.setAttribute('data-wcl-comment-id',c.id);el.setAttribute('data-wcl-comment',c.body);
  const pin=document.createElement('div');pin.className='wcl-pin';pin.textContent='✓';pin.title=c.body;
  pin.onclick=ev=>{ev.stopPropagation();ev.preventDefault();showComment(el,c.id,c.body);};
  el.appendChild(pin);
 }
}
function showComment(el,id,body){
 closePop();const r=el.getBoundingClientRect();
 pop=document.createElement('div');pop.className='wcl-pop';
 pop.style.top=(window.scrollY+Math.max(8,r.top)+24)+'px';
 pop.style.left=(window.scrollX+Math.max(8,Math.min(r.left,window.innerWidth-320)))+'px';
 pop.innerHTML=`<div>${esc(body)}</div><div class="row"><button class="ghost" data-x>Close</button><button data-r>Resolve</button></div>`;
 document.body.appendChild(pop);
 pop.querySelector('[data-x]').onclick=closePop;
 pop.querySelector('[data-r]').onclick=async()=>{
  if(await post({resolve_id:id},pop)){closePop();await refresh();}
 };
}

// Comments on *this* page: fetched from the sidecar(s) and filtered by the
// page key (name + source file), which disambiguates same-named pages across
// different sites / wskills.
let allComments=[];
function pageComments(){
 if(!pageEl)return [];
 const pname=pageEl.getAttribute('data-wcl-page-name');
 const pf=pageEl.getAttribute('data-wcl-page-file');
 return allComments.filter(c=>c.page===pname&&(!c.page_file||c.page_file===pf));
}
async function refresh(){
 try{const r=await fetch('/__wdoc_comment');allComments=r.ok?await r.json():[];}catch(_){allComments=[];}
 placePins();
 const n=pageComments().length;
 countBtn.textContent='💬 '+n+(n===1?' comment':' comments');
 updateBadge();
}

// Jump-to works for any block comment whose locator resolves on this page.
function elForComment(c){return c.loc?elByLoc(c.loc):null;}
function jumpTo(c){
 const el=elForComment(c);if(!el)return;
 el.scrollIntoView({behavior:'smooth',block:'center'});
 el.classList.add('wcl-flash');setTimeout(()=>el.classList.remove('wcl-flash'),1600);
}

let modal=null;
function closeModal(){if(modal){modal.remove();modal=null;}}
function openModal(){
 closeModal();
 const items=pageComments();
 modal=document.createElement('div');modal.className='wcl-modal';
 const box=document.createElement('div');box.className='wcl-modal-box';
 modal.appendChild(box);
 const render=()=>{
  const list=pageComments();
  box.innerHTML='<div class="wcl-modal-h"><span>Comments on this page ('+list.length+')</span><button data-x>Close</button></div>';
  if(!list.length){const e=document.createElement('div');e.className='wcl-empty';e.textContent='No comments on this page yet.';box.appendChild(e);}
  for(const c of list){
   const where=c.scope==='page'?'Whole page':(c.target||'block');
   const who=c.author?(' · '+c.author):'';
   const div=document.createElement('div');div.className='wcl-c';
   const head='<div class="meta">'+esc(where)+esc(who)+'</div>'+(c.quote?'<div class="q">'+esc(c.quote)+'</div>':'');
   const view=()=>{
    div.innerHTML=head+'<div>'+esc(c.body)+'</div>'+
      '<div class="acts">'+(elForComment(c)?'<button class="ghost" data-j>Jump to</button>':'')+
      '<button class="ghost" data-e>Edit</button><button data-r>Resolve</button></div>';
    const j=div.querySelector('[data-j]');if(j)j.onclick=()=>{closeModal();jumpTo(c);};
    div.querySelector('[data-e]').onclick=edit;
    div.querySelector('[data-r]').onclick=async()=>{if(await post({resolve_id:c.id},box)){await refresh();render();}};
   };
   const edit=()=>{
    div.innerHTML=head+'<textarea></textarea>'+
      '<div class="acts"><button class="ghost" data-x>Cancel</button><button data-s>Save</button></div>';
    const ta=div.querySelector('textarea');ta.value=c.body;ta.focus();
    div.querySelector('[data-x]').onclick=view;
    div.querySelector('[data-s]').onclick=async()=>{
     const nb=ta.value.trim();if(!nb)return;
     if(await post({edit_id:c.id,body:nb},box)){await refresh();render();}
    };
   };
   view();
   box.appendChild(div);
  }
  box.querySelector('[data-x]').onclick=closeModal;
 };
 render();
 document.body.appendChild(modal);
 modal.addEventListener('click',e=>{if(e.target===modal)closeModal();});
}

const bar=document.createElement('div');bar.className='wcl-bar';
const actions=document.createElement('div');actions.className='wcl-actions';
const countBtn=document.createElement('button');countBtn.className='wcl-count';countBtn.textContent='💬 …';
countBtn.onclick=()=>{setOpen(false);openModal();};
const selBtn=document.createElement('button');selBtn.textContent='🎯 Comment on a block';
selBtn.onclick=()=>{setOpen(false);setPick(!picking);};
const pageBtn=document.createElement('button');pageBtn.textContent='💬 Comment on page';
pageBtn.onclick=()=>{setOpen(false);setPick(false);if(pageEl)openForm(pageEl,true);};
// A top banner shows rebuild progress; a done note is stashed so it survives
// the post-rebuild reload and shows on the fresh page.
let rbBanner=null,rbTimer=null;
function showRebuild(kind,text){
 if(!rbBanner){rbBanner=document.createElement('div');document.body.appendChild(rbBanner);}
 rbBanner.className='wcl-rebuild wcl-rb-'+kind;
 rbBanner.innerHTML=(kind==='running'?'<span class="wcl-rb-spin"></span>':'')+esc(text);
 if(rbTimer){clearTimeout(rbTimer);rbTimer=null;}
 if(kind!=='running'){rbTimer=setTimeout(()=>{if(rbBanner){rbBanner.remove();rbBanner=null;}},4000);}
}
// Auto-rebuild is off server-side; this asks the server to rebuild now. With a
// page in scope it rebuilds only that page's sub-site; the call blocks until the
// build finishes (so the spinner spans it), then the reload script refreshes.
const rebuildBtn=document.createElement('button');rebuildBtn.textContent='🔁 Rebuild';
rebuildBtn.onclick=async()=>{
 setOpen(false);rebuildBtn.disabled=true;
 const pf=pageEl?pageEl.getAttribute('data-wcl-page-file'):null;
 showRebuild('running','Rebuilding'+(pf?' this page…':'…'));
 let res=null;
 try{const r=await fetch('/__wdoc_rebuild',{method:'POST',headers:{'content-type':'application/json'},
   body:JSON.stringify({page_file:pf})});res=await r.json();}catch(e){res={ok:false,summary:String(e)};}
 rebuildBtn.disabled=false;
 if(res&&res.ok){
  // Stash the done note; the reload (gen bumped by the build) shows it next load.
  try{sessionStorage.setItem('wcl-rebuilt',res.summary||'done');}catch(_){ }
  showRebuild('done','✓ Rebuilt '+(res.summary||''));
 }else{showRebuild('error','⚠ '+((res&&res.summary)||'rebuild failed'));}
};
if(pageEl)actions.appendChild(countBtn);
actions.appendChild(rebuildBtn);
actions.appendChild(selBtn);if(pageEl)actions.appendChild(pageBtn);
// Persistent launcher: collapsed by default, expands the action buttons.
const toggleBtn=document.createElement('button');toggleBtn.className='wcl-toggle';toggleBtn.title='Review tools';
const badge=document.createElement('span');badge.className='wcl-badge';
toggleBtn.appendChild(document.createTextNode('💬'));toggleBtn.appendChild(badge);
function setOpen(open){bar.classList.toggle('wcl-open',open);toggleBtn.firstChild.textContent=open?'✕':'💬';}
toggleBtn.onclick=()=>setOpen(!bar.classList.contains('wcl-open'));
function updateBadge(){const n=pageEl?pageComments().length:0;
 badge.textContent=n>99?'99+':String(n);badge.classList.toggle('on',n>0);}
bar.appendChild(actions);bar.appendChild(toggleBtn);
document.body.appendChild(bar);
// Show the "rebuilt" note carried across the post-rebuild reload.
try{const done=sessionStorage.getItem('wcl-rebuilt');if(done!==null){sessionStorage.removeItem('wcl-rebuilt');showRebuild('done','✓ Rebuilt '+done);}}catch(_){ }
refresh();

// Review handshake: a `wcl wdoc review` agent waiting for the reviewer shows a
// banner inviting them to rebuild, comment, and send. The status long-poll
// re-surfaces it each round, so when the agent finishes its changes and waits
// again the banner reappears as the "agent is done" notification.
let reviewBar=null,reviewRound=0;
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
function showReview(on){
 if(on){
  if(reviewBar)return;
  reviewBar=document.createElement('div');reviewBar.className='wcl-review-bar';
  reviewBar.innerHTML='<span>🤖 An AI agent is ready for your review — rebuild to see its latest changes, leave comments, then send.</span>'+
    '<button class="ghost wcl-rv-rebuild">🔁 Rebuild</button><button class="wcl-rv-send">✅ Send to agent</button>';
  document.body.appendChild(reviewBar);
  reviewBar.querySelector('.wcl-rv-rebuild').onclick=()=>rebuildBtn.onclick();
  reviewBar.querySelector('.wcl-rv-send').onclick=async()=>{
   const b=reviewBar.querySelector('.wcl-rv-send');b.disabled=true;b.textContent='Sending…';
   try{await fetch('/__wdoc_review/ready',{method:'POST'});}catch(_){ }
   showReview(false);
  };
 }else if(reviewBar){reviewBar.remove();reviewBar=null;}
}
async function pollReview(){
 for(;;){
  let j=null;
  try{const r=await fetch('/__wdoc_review/status?round='+reviewRound);if(r.ok)j=await r.json();}catch(_){ }
  if(!j){await sleep(2000);continue;}
  reviewRound=j.round||0;
  showReview(!!j.waiting);
 }
}
pollReview();
})();"#;

/// Shared between the rebuild loop and the request handlers.
struct ServeState {
    out: PathBuf,
    /// Plain-text rendering of the most recent failed build; `None`
    /// when the last build succeeded. While set, HTML requests get a
    /// build-failure page instead of the stale previous build.
    error: RwLock<Option<String>>,
    /// Bumped after every build attempt — success *and* failure — so
    /// a browser parked on the error page reloads when the fix lands.
    generation: tokio::sync::watch::Sender<u64>,
    /// Comment mode (`--comment`): inject the comment client into HTML,
    /// expose the `/__wdoc_comment` endpoints, and build with `data-wcl-*`
    /// anchors.
    comment_mode: bool,
    /// Edit mode (`--edit`): inject the WYSIWYG editor client into HTML,
    /// expose the `/__wdoc_edit*` / `/__wdoc_object*` endpoints, and build
    /// with `data-wcl-span` / `data-wcl-file` anchors. Composes with
    /// `comment_mode` — both clients can be injected at once.
    edit_mode: bool,
    /// Answer mode (`--answer`): inject the questionnaire client into HTML
    /// and expose the `/__wdoc_answers` / `/__wdoc_answer` endpoints — the
    /// respondent-facing walk-through of `@answerable` question blocks.
    /// Composes with both other modes.
    answer_mode: bool,
    /// Watched source root; `comments.wcl` sidecars are discovered under it and
    /// comment / edit writes are sandboxed to within it.
    watch_root: PathBuf,
    /// The document entry-point file passed to `serve` — the root `.wcl` the
    /// editor reopens to introspect schemas and resolve object instances.
    root_file: PathBuf,
    /// Source `.wcl` files changed since the last rebuild. The watcher
    /// accumulates here instead of rebuilding; a rebuild is triggered manually
    /// (Enter in the console, or the toolbar's Rebuild button) and drains this.
    pending: Mutex<Vec<PathBuf>>,
    /// Send a [`RebuildReq`] to request a rebuild. The console (stdin Enter)
    /// and the `/__wdoc_rebuild` endpoint both use it; the rebuild worker runs
    /// one build per request and (when asked) reports completion back.
    rebuild_tx: tokio::sync::mpsc::UnboundedSender<RebuildReq>,
    /// Review handshake markers (comment mode only): the file-based coordination
    /// with a `wcl wdoc review` process. `None` when `--comment` is off.
    review: Option<wcl_wdoc::Handshake>,
    /// Preview-without-saving scratch state (edit mode only): the temp output
    /// tree the `/__wdoc_preview` endpoints render into and serve from.
    preview: Option<crate::preview::Preview>,
}

/// A rebuild request handed to the rebuild worker.
struct RebuildReq {
    /// The page the request came from (the toolbar Rebuild button), used to
    /// scope the rebuild to that page's included sub-site. `None` for a console
    /// (Enter) rebuild, which rebuilds the whole served site.
    page_file: Option<PathBuf>,
    /// Resolved when the build finishes, so the HTTP handler can report the
    /// result (and time its progress spinner). `None` for the console path.
    done: Option<tokio::sync::oneshot::Sender<RebuildReport>>,
}

/// What a rebuild did, returned to the Rebuild button for its status display.
struct RebuildReport {
    ok: bool,
    /// What was rebuilt — `"site"` or the sub-site's output subdir.
    scope: String,
    /// Human summary (page count, or the first line of the error).
    summary: String,
}

/// Print any non-fatal edge warnings left by the most recent build (edges
/// whose endpoint matched no shape id) to stderr.
fn print_edge_warnings() {
    for w in wcl_wdoc::take_render_warnings() {
        eprintln!("warning: {w}");
    }
}

/// Run one build, report to stderr, record the outcome in `state`, and
/// bump the live-reload generation.
fn run_build(file: &Path, out: &Path, site: Option<&str>, state: &ServeState, rebuild: bool) {
    let opts = BuildOptions {
        comment_mode: state.comment_mode,
        edit_mode: state.edit_mode,
        ..Default::default()
    };
    match build_with_options(file, out, site, &opts).map(|(n, _)| n) {
        Ok(n) => {
            print_edge_warnings();
            let plural = if n == 1 { "" } else { "s" };
            if rebuild {
                eprintln!("rebuilt: {n} page{plural}");
            } else {
                eprintln!("rendered {n} page{plural}");
            }
            *state.error.write().unwrap_or_else(|e| e.into_inner()) = None;
        }
        Err(err) => {
            eprintln!(
                "{} failed:",
                if rebuild { "rebuild" } else { "initial build" }
            );
            err.report();
            *state.error.write().unwrap_or_else(|e| e.into_inner()) = Some(err.render_plain());
        }
    }
    state.generation.send_modify(|g| *g += 1);
}

/// Handle one [`RebuildReq`]. When the request names a page that belongs to an
/// included sub-site (e.g. a wskill), rebuild **only** that sub-site into its
/// output subdir, draining just the pending changes under it — so the Rebuild
/// button on a sub-site page is fast and scoped. Otherwise (console Enter, or a
/// root page) drain all pending and rebuild the top-level site. Records the
/// outcome in `state`, bumps the live-reload generation, and returns a report.
fn run_rebuild_request(
    file: &Path,
    out: &Path,
    site: Option<&str>,
    state: &ServeState,
    page_file: Option<PathBuf>,
) -> RebuildReport {
    let opts = BuildOptions {
        comment_mode: state.comment_mode,
        edit_mode: state.edit_mode,
        ..Default::default()
    };

    // Scope to the page's sub-site when the request names one.
    if let Some(pf) = page_file.as_deref()
        && let Some(sub) = wcl_wdoc::subsite_for_page(file, pf)
    {
        let changed = drain_pending_under(state, Some(&sub.src_root));
        let sub_out = out.join(&sub.out_subdir);
        let scope = sub.out_subdir.display().to_string();
        let result = build_incremental(&sub.entry, &sub_out, sub.site.as_deref(), &opts, &changed);
        return finish_rebuild(state, scope, result.map(rebuild_summary));
    }

    // Whole-site rebuild: drain everything pending; full when nothing's pending.
    let changed = drain_pending_under(state, None);
    let result = if changed.is_empty() {
        build_with_options(file, out, site, &opts).map(|(n, _)| format!("{} (full)", page_count(n)))
    } else {
        build_incremental(file, out, site, &opts, &changed).map(rebuild_summary)
    };
    finish_rebuild(state, "site".to_string(), result)
}

/// Drain pending changed paths: those under `scope` (a sub-site source root),
/// leaving the rest queued; or *all* of them when `scope` is `None`.
fn drain_pending_under(state: &ServeState, scope: Option<&Path>) -> Vec<PathBuf> {
    let mut g = state.pending.lock().unwrap_or_else(|e| e.into_inner());
    let mut taken = match scope {
        None => std::mem::take(&mut *g),
        Some(root) => {
            let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
            let (under, rest): (Vec<_>, Vec<_>) =
                std::mem::take(&mut *g).into_iter().partition(|p| {
                    std::fs::canonicalize(p)
                        .map(|c| c.starts_with(&root))
                        .unwrap_or(false)
                });
            *g = rest;
            under
        }
    };
    taken.sort();
    taken.dedup();
    taken
}

/// Record a build outcome in `state`, bump the live-reload generation, and
/// build the [`RebuildReport`]. `result` is `Ok(summary)` or the build error.
fn finish_rebuild(
    state: &ServeState,
    scope: String,
    result: Result<String, wcl_wdoc::BuildError>,
) -> RebuildReport {
    let report = match result {
        Ok(summary) => {
            print_edge_warnings();
            eprintln!("rebuilt {scope}: {summary}");
            *state.error.write().unwrap_or_else(|e| e.into_inner()) = None;
            RebuildReport {
                ok: true,
                scope,
                summary,
            }
        }
        Err(err) => {
            eprintln!("rebuild failed ({scope}):");
            err.report();
            let plain = err.render_plain();
            *state.error.write().unwrap_or_else(|e| e.into_inner()) = Some(plain.clone());
            RebuildReport {
                ok: false,
                scope,
                summary: plain.lines().next().unwrap_or("build failed").to_string(),
            }
        }
    };
    state.generation.send_modify(|g| *g += 1);
    report
}

/// A one-line summary of an incremental rebuild outcome.
fn rebuild_summary(outcome: RebuildOutcome) -> String {
    match outcome {
        RebuildOutcome::Targeted { pages } => {
            format!("{} ({})", page_count(pages.len()), pages.join(", "))
        }
        RebuildOutcome::Full { pages } => format!("{} (full)", page_count(pages)),
    }
}

/// `"1 page"` / `"3 pages"`.
fn page_count(n: usize) -> String {
    format!("{n} page{}", if n == 1 { "" } else { "s" })
}

pub(crate) async fn serve(
    file: PathBuf,
    out: Option<PathBuf>,
    addr: BindSpec,
    site: Option<String>,
    comment_mode: bool,
    edit_mode: bool,
    answer_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Resolve the output directory. If `--out` wasn't given, create a
    // TempDir and hold it for the lifetime of `serve` so cleanup runs
    // when the future is dropped (Ctrl-C / shutdown).
    let (out_dir, _tempdir_guard): (PathBuf, Option<TempDir>) = match out {
        Some(p) => {
            std::fs::create_dir_all(&p)?;
            (p, None)
        }
        None => {
            let td = tempfile::Builder::new().prefix("wdoc-").tempdir()?;
            (td.path().to_path_buf(), Some(td))
        }
    };

    // Hard stop on Ctrl-C. A *dedicated* task owns the kill so the signal is
    // observed even while the watch loop is mid-rebuild: `run_build` is a
    // synchronous, non-cancellable call that blocks its worker thread, so a
    // shutdown branch sharing the `select!` task below would never get polled
    // until the build finished. `process::exit` tears down every thread at
    // once (the inotify watcher, axum connections, parked reload long-polls,
    // and any in-flight build) and, by ending the process, blocks all further
    // rebuilds. It skips the TempDir guard's `Drop`, so clean the temp output
    // dir by hand first.
    // Review handshake (comment mode only): publish a "server is live" marker
    // so a `wcl wdoc review` process can hand off to this server, and clear it
    // on shutdown.
    let review = if comment_mode {
        let hs = wcl_wdoc::Handshake::new(&file);
        if let Err(e) = hs.serve_started() {
            eprintln!("warning: could not initialise review handshake: {e}");
        }
        Some(hs)
    } else {
        None
    };

    let temp_cleanup = _tempdir_guard.as_ref().map(|td| td.path().to_path_buf());
    let review_cleanup = review.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nshutting down");
        if let Some(hs) = &review_cleanup {
            hs.serve_stopped();
        }
        if let Some(p) = temp_cleanup {
            let _ = std::fs::remove_dir_all(&p);
        }
        std::process::exit(0);
    });

    let watch_root = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    // Rebuilds are triggered manually (not on every file change). Both the
    // console (stdin Enter) and the `/__wdoc_rebuild` endpoint send on this.
    let (rebuild_tx, mut rebuild_rx) = tokio::sync::mpsc::unbounded_channel::<RebuildReq>();

    let state = Arc::new(ServeState {
        out: out_dir.clone(),
        error: RwLock::new(None),
        generation: tokio::sync::watch::Sender::new(0),
        comment_mode,
        edit_mode,
        answer_mode,
        watch_root: watch_root.clone(),
        root_file: file.clone(),
        pending: Mutex::new(Vec::new()),
        rebuild_tx: rebuild_tx.clone(),
        review,
        preview: if edit_mode {
            Some(crate::preview::Preview::new()?)
        } else {
            None
        },
    });

    // Initial build. Failure is non-fatal — HTML requests serve the
    // build-failure page until the next (manual) rebuild succeeds.
    run_build(&file, &out_dir, site.as_deref(), &state, false);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })?;
    watcher.watch(&watch_root, RecursiveMode::Recursive)?;

    // Console driver: pressing Enter requests a rebuild. A blocking thread (not
    // an async stdin reader, which can stall process exit) feeds the trigger
    // channel; EOF (no interactive console — piped/closed stdin) just ends the
    // thread so non-interactive runs don't spin.
    {
        let trigger = rebuild_tx.clone();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            let mut line = String::new();
            loop {
                line.clear();
                match stdin.read_line(&mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        // A console Enter rebuilds the whole served site.
                        if trigger
                            .send(RebuildReq {
                                page_file: None,
                                done: None,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });
    }

    let bg_state = Arc::clone(&state);
    // The watcher no longer rebuilds — it accumulates the changed `.wcl` paths
    // into `state.pending` and notifies the console. A *local* future (not a
    // detached task) so it drops with `serve`, stopping the inotify thread.
    let watch_loop = async move {
        let _watcher = watcher; // keep the watcher alive for this future's lifetime
        while let Some(event) = rx.recv().await {
            if !is_relevant(&event) {
                continue;
            }
            // Coalesce the notify event storm one save fires into a single note.
            let mut changed: Vec<PathBuf> = wcl_paths(&event);
            drain_quiet(&mut rx, &mut changed).await;
            let n = changed.len();
            bg_state
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend(changed);
            eprintln!(
                "{n} file change{} pending — press Enter (or click Rebuild) to rebuild",
                if n == 1 { "" } else { "s" }
            );
        }
    };

    let rb_file = file.clone();
    let rb_out = out_dir.clone();
    let rb_site = site.clone();
    let rb_state = Arc::clone(&state);
    // Rebuild worker: one build per request. Scopes to the request's sub-site
    // when given (the Rebuild button on a sub-site page), else rebuilds the
    // whole site (console Enter). Reports completion back when asked.
    let rebuild_loop = async move {
        while let Some(req) = rebuild_rx.recv().await {
            let report = run_rebuild_request(
                &rb_file,
                &rb_out,
                rb_site.as_deref(),
                &rb_state,
                req.page_file,
            );
            if let Some(done) = req.done {
                let _ = done.send(report);
            }
        }
    };

    // One generic static handler resolves any request path against the
    // output tree, so it serves both the flat single-site layout and the
    // nested multi-site one (`/<site>/…`, `/<site>/_wdoc/…`) plus the
    // generated chooser at `/`, with no per-route knowledge. The reload
    // endpoint sits outside the output tree's namespace.
    let mut app = Router::new()
        .route("/__wdoc_reload", get(handle_reload))
        .route("/__wdoc_rebuild", axum::routing::post(handle_rebuild));
    if comment_mode {
        app = app
            .route("/__wdoc_comment.js", get(handle_comment_js))
            .route(
                "/__wdoc_comment",
                get(handle_comment_list).post(handle_comment_post),
            )
            .route("/__wdoc_review/status", get(handle_review_status))
            .route(
                "/__wdoc_review/ready",
                axum::routing::post(handle_review_ready),
            );
    }
    if edit_mode {
        app = app
            .route("/__wdoc_edit.js", get(handle_edit_js))
            .route("/__wdoc_editor.js", get(handle_editor_js))
            .route(
                "/__wdoc_highlight",
                axum::routing::post(handle_editor_highlight),
            )
            .route("/__wdoc_check", axum::routing::post(handle_editor_check))
            .route("/__wdoc_format", axum::routing::post(handle_editor_format))
            .route("/__wdoc_files", get(handle_files_list))
            .route(
                "/__wdoc_file",
                get(handle_file_read).post(handle_file_write),
            )
            .route("/__wdoc_preview", axum::routing::post(handle_preview_build))
            .route("/__wdoc_preview/{*path}", get(handle_preview_file))
            .route("/__wdoc_schema", get(handle_edit_schema))
            .route("/__wdoc_object_kinds", get(handle_object_kinds))
            .route("/__wdoc_objects", get(handle_object_instances))
            .route("/__wdoc_object_source", get(handle_object_source))
            .route("/__wdoc_object_template", get(handle_object_template))
            .route(
                "/__wdoc_object",
                get(handle_read_object).post(handle_object_post),
            )
            .route("/__wdoc_edit/field", axum::routing::post(handle_edit_field))
            .route("/__wdoc_edit/add", axum::routing::post(handle_edit_add))
            .route(
                "/__wdoc_edit/delete",
                axum::routing::post(handle_edit_delete),
            )
            .route("/__wdoc_edit/move", axum::routing::post(handle_edit_move));
    }
    if answer_mode {
        app = app
            .route("/__wdoc_answer.js", get(handle_answer_js))
            .route("/__wdoc_answers", get(handle_answers_list))
            .route("/__wdoc_answer", axum::routing::post(handle_answer_post));
    }
    let app = app
        .fallback(get(handle_static))
        .with_state(Arc::clone(&state))
        .layer(middleware::from_fn(log_requests));

    let listener = match addr {
        BindSpec::Auto => bind_auto(DEFAULT_BIND).await?,
        BindSpec::Fixed(a) => tokio::net::TcpListener::bind(a).await?,
    };
    let bound = listener.local_addr()?;
    println!(
        "serving http://{bound}  (source: {}, out: {})",
        file.display(),
        out_dir.display()
    );
    println!("auto-rebuild is off — press Enter here (or click Rebuild) to rebuild after edits");

    // Run the server, the watcher, and the rebuild worker concurrently. None
    // completes in normal operation — they run until the Ctrl-C task above
    // hard-exits the process. No graceful shutdown: a parked reload long-poll
    // (up to `POLL_TIMEOUT`) must not delay teardown.
    tokio::select! {
        res = axum::serve(listener, app).into_future() => res?,
        _ = watch_loop => {}
        _ = rebuild_loop => {}
    }
    Ok(())
}

/// Keep draining the event channel until `QUIET_WINDOW` passes with no
/// further *relevant* event. Irrelevant events (e.g. the build's own
/// output writes when `--out` sits inside the watched tree) are
/// swallowed without extending the window.
async fn drain_quiet(rx: &mut UnboundedReceiver<Event>, changed: &mut Vec<PathBuf>) {
    let mut deadline = tokio::time::Instant::now() + QUIET_WINDOW;
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(ev)) if is_relevant(&ev) => {
                changed.extend(wcl_paths(&ev));
                deadline = tokio::time::Instant::now() + QUIET_WINDOW;
            }
            Ok(Some(_)) => {}
            // Window elapsed quiet, or the channel closed (the outer
            // loop's recv will observe the closure).
            Err(_) | Ok(None) => break,
        }
    }
}

fn is_relevant(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    ) && event.paths.iter().any(|p| is_source_wcl(p))
}

/// A `.wcl` path that is part of the document — i.e. not a `comments.wcl`
/// sidecar, whose writes must never trigger a rebuild (comments render
/// client-side, so the build doesn't read it).
fn is_source_wcl(p: &Path) -> bool {
    p.extension().is_some_and(|e| e == "wcl")
        && p.file_name().and_then(|n| n.to_str()) != Some("comments.wcl")
}

/// The document `.wcl` paths an event touched (the granularity
/// `build_incremental` maps onto pages); `comments.wcl` sidecars are excluded.
fn wcl_paths(event: &Event) -> Vec<PathBuf> {
    event
        .paths
        .iter()
        .filter(|p| is_source_wcl(p))
        .cloned()
        .collect()
}

/// Live-reload long-poll. Without `?gen=`, answers immediately with the
/// current build generation. With `?gen=N` matching the current value,
/// parks until the next build attempt (or `POLL_TIMEOUT`) and answers
/// with the then-current generation; the client reloads on a change.
async fn handle_reload(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let asked: Option<u64> = uri
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("gen=")))
        .and_then(|v| v.parse().ok());
    if asked == Some(*state.generation.borrow()) {
        let mut rx = state.generation.subscribe();
        let _ = tokio::time::timeout(POLL_TIMEOUT, rx.changed()).await;
    }
    let current = *state.generation.borrow();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        current.to_string(),
    )
        .into_response()
}

/// Request a rebuild (the toolbar's Rebuild button). Scopes to the current
/// page's sub-site (from the optional `page_file` body field) and **waits** for
/// the build to finish, so the button can show a running/done indication; the
/// reload long-poll then reloads the page when the generation bumps. A missing
/// `page_file` (or a root page) rebuilds the whole site.
async fn handle_rebuild(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let page_file = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| {
            v.get("page_file")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .filter(|s| !s.is_empty())
        .and_then(|f| sandboxed(&state.watch_root, Path::new(&f)));

    let (tx, rx) = tokio::sync::oneshot::channel();
    if state
        .rebuild_tx
        .send(RebuildReq {
            page_file,
            done: Some(tx),
        })
        .is_err()
    {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "rebuild worker is gone");
    }
    match rx.await {
        Ok(report) => json_response(
            StatusCode::OK,
            &serde_json::json!({
                "ok": report.ok,
                "scope": report.scope,
                "summary": report.summary,
            }),
        ),
        Err(_) => json_error(StatusCode::INTERNAL_SERVER_ERROR, "rebuild was cancelled"),
    }
}

/// Review-handshake status long-poll (the comment toolbar). The client passes
/// its last-known wait round as `?round=N` (0 = not waiting). While the current
/// state matches, the request parks up to `POLL_TIMEOUT`, polling the `agent`
/// marker; it answers `{waiting, round}` as soon as the round changes (a fresh
/// `wcl wdoc review` wait, or its end) or the window elapses. A new round each
/// time `review` runs is what re-shows the banner after the agent's changes.
async fn handle_review_status(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let asked: u64 = uri
        .query()
        .and_then(|q| q.split('&').find_map(|kv| kv.strip_prefix("round=")))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let Some(hs) = &state.review else {
        return json_response(
            StatusCode::OK,
            &serde_json::json!({ "waiting": false, "round": 0 }),
        );
    };
    let deadline = tokio::time::Instant::now() + POLL_TIMEOUT;
    let mut current = hs.agent_waiting().unwrap_or(0);
    while current == asked && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        current = hs.agent_waiting().unwrap_or(0);
    }
    json_response(
        StatusCode::OK,
        &serde_json::json!({ "waiting": current != 0, "round": current }),
    )
}

/// Release a blocked `wcl wdoc review` (the toolbar's "Send to agent" button):
/// write the `ready` marker the review process is polling for.
async fn handle_review_ready(State(state): State<Arc<ServeState>>) -> Response {
    let Some(hs) = &state.review else {
        return json_error(StatusCode::BAD_REQUEST, "review handshake is not active");
    };
    match hs.signal_ready() {
        Ok(()) => json_response(StatusCode::OK, &serde_json::json!({ "ok": true })),
        Err(e) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("could not signal the agent: {e}"),
        ),
    }
}

/// Serve the comment-mode client script.
async fn handle_comment_js() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        COMMENT_CLIENT_JS,
    )
        .into_response()
}

/// List every stored comment as JSON, for the comment client to re-show them.
async fn handle_comment_list(State(state): State<Arc<ServeState>>) -> Response {
    match comments::list(&state.watch_root) {
        Ok(recs) => {
            let arr = serde_json::Value::Array(recs.iter().map(comment_to_json).collect());
            json_response(StatusCode::OK, &arr)
        }
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.render_plain()),
    }
}

/// Add a comment (or, with `resolve_id`, delete one; with `edit_id` + `body`,
/// edit one). The body is a JSON object — see `COMMENT_CLIENT_JS` for the shapes.
async fn handle_comment_post(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let v: serde_json::Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("bad json: {e}")),
    };
    let str_of = |k: &str| v.get(k).and_then(serde_json::Value::as_str);

    // Resolve (delete) an existing comment by id.
    if let Some(id) = str_of("resolve_id") {
        return match comments::resolve(&state.watch_root, id) {
            Ok(true) => json_response(StatusCode::OK, &serde_json::json!({ "resolved": id })),
            Ok(false) => json_error(StatusCode::NOT_FOUND, "no such comment"),
            Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.render_plain()),
        };
    }

    let Some(body_text) = str_of("body").filter(|s| !s.trim().is_empty()) else {
        return json_error(StatusCode::BAD_REQUEST, "missing comment body");
    };

    // Edit an existing comment's body by id.
    if let Some(id) = str_of("edit_id") {
        return match comments::edit(&state.watch_root, id, body_text) {
            Ok(true) => json_response(StatusCode::OK, &serde_json::json!({ "edited": id })),
            Ok(false) => json_error(StatusCode::NOT_FOUND, "no such comment"),
            Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.render_plain()),
        };
    }

    // Add: route to the comments.wcl sidecar that owns the page (beside the
    // page's wskill, else the served root). `page_file` is the page's source.
    let Some(page) = str_of("page").filter(|s| !s.is_empty()) else {
        return json_error(StatusCode::BAD_REQUEST, "missing page");
    };
    let page_file = str_of("page_file");
    // Sandbox the page's source file, then derive the sidecar from it (the
    // sidecar path is always within the served root).
    let sidecar = match page_file.and_then(|f| sandboxed(&state.watch_root, Path::new(f))) {
        Some(pf) => comments::comments_path(&pf, &state.watch_root),
        None => state.watch_root.join("comments.wcl"),
    };
    match comments::add(
        &sidecar,
        page,
        page_file,
        str_of("loc"),
        str_of("target"),
        body_text,
        str_of("author"),
        str_of("quote"),
    ) {
        Ok(id) => json_response(StatusCode::OK, &serde_json::json!({ "id": id })),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.render_plain()),
    }
}

// --- WYSIWYG editor (`--edit`) handlers. Thin wrappers over `crate::edit`. ---

/// Serve the WYSIWYG editor client script.
async fn handle_edit_js() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        crate::edit::EDIT_CLIENT_JS,
    )
        .into_response()
}

/// Serve the shared source-editor component script (`WclEditor`).
async fn handle_editor_js() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        crate::edit::EDITOR_CLIENT_JS,
    )
        .into_response()
}

/// `POST /__wdoc_highlight` — highlight a buffer for the editor backdrop.
async fn handle_editor_highlight(body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    edit_result(crate::edit::highlight_source(&v))
}

/// `POST /__wdoc_format` — canonically format a buffer (`wcl fmt` core).
async fn handle_editor_format(body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    edit_result(crate::edit::format_source(&v))
}

/// `POST /__wdoc_check` — dry-run syntax + schema diagnostics for a buffer.
async fn handle_editor_check(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    edit_result(crate::edit::check_source(
        &state.root_file,
        &state.watch_root,
        &v,
    ))
}

/// `GET /__wdoc_files` — the source-editor file tree (page-scoped).
async fn handle_files_list(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    edit_result(crate::edit::list_files(
        &state.root_file,
        &state.watch_root,
        query_param(&uri, "page_file").as_deref(),
    ))
}

/// `GET /__wdoc_file?path=` — a source file's text + etag.
async fn handle_file_read(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let Some(path) = query_param(&uri, "path") else {
        return json_error(StatusCode::BAD_REQUEST, "missing path");
    };
    edit_result(crate::edit::read_file(&state.watch_root, &path))
}

/// `POST /__wdoc_file` — whole-file save (validate → write → rollback).
async fn handle_file_write(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let entry = entry_for(
        &state,
        v.get("page_file").and_then(serde_json::Value::as_str),
    );
    edit_result(crate::edit::write_file(&state.watch_root, &entry, &v))
}

/// `POST /__wdoc_preview` — render unsaved buffers into the scratch tree and
/// return the preview URL of the current page. Serialized (previews coalesce
/// behind one gate) and run off the async executor — a render is real work.
async fn handle_preview_build(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let Some(preview) = &state.preview else {
        return json_error(StatusCode::BAD_REQUEST, "preview requires --edit");
    };
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let _gate = preview.lock().await;
    let state2 = state.clone();
    let result = tokio::task::spawn_blocking(move || {
        let preview = state2
            .preview
            .as_ref()
            .expect("preview state checked above");
        crate::preview::preview_build(preview, &state2.root_file, &state2.watch_root, &v)
    })
    .await
    .unwrap_or_else(|e| Err(format!("preview task failed: {e}")));
    edit_result(result)
}

/// `GET /__wdoc_preview/{*path}` — serve a rendered preview file from the
/// scratch tree, with **no** reload/comment/edit scripts injected (a preview
/// iframe must neither live-reload nor recurse the editor chrome).
async fn handle_preview_file(
    State(state): State<Arc<ServeState>>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Response {
    let Some(preview) = &state.preview else {
        return json_error(StatusCode::BAD_REQUEST, "preview requires --edit");
    };
    // Resolve inside the scratch tree only (reject traversal).
    let rel = Path::new(&path);
    if rel
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return json_error(StatusCode::BAD_REQUEST, "bad preview path");
    }
    let file = preview.root().join(rel);
    match tokio::fs::read(&file).await {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type(&file))],
            bytes,
        )
            .into_response(),
        Err(_) => json_error(StatusCode::NOT_FOUND, "no such preview file"),
    }
}

/// Read the `kind` query parameter (`?kind=...`), URL-decoding `%xx` / `+`.
fn query_param(uri: &Uri, key: &str) -> Option<String> {
    uri.query()?.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| url_decode(v))
    })
}

/// Minimal `application/x-www-form-urlencoded` decode for query values.
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The document entry the editor should introspect for a request: the included
/// sub-site (e.g. a wskill) that owns `page_file`, else the served root. Lets
/// `--edit` on the top-level site edit a sub-site's objects/schema.
fn entry_for(state: &ServeState, page_file: Option<&str>) -> PathBuf {
    match page_file
        .filter(|s| !s.is_empty())
        .and_then(|f| sandboxed(&state.watch_root, Path::new(f)))
    {
        Some(pf) => wcl_wdoc::doc_entry_for_page(&state.root_file, &pf),
        None => state.root_file.clone(),
    }
}

/// Map a `Result<json, message>` from `crate::edit` to an HTTP response.
fn edit_result(r: Result<serde_json::Value, String>) -> Response {
    match r {
        Ok(v) => json_response(StatusCode::OK, &v),
        Err(e) => json_error(StatusCode::BAD_REQUEST, &e),
    }
}

/// Parse a JSON request body, or an error message on malformed JSON.
fn parse_json_body(body: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(body).map_err(|e| format!("bad json: {e}"))
}

async fn handle_edit_schema(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let Some(kind) = query_param(&uri, "kind") else {
        return json_error(StatusCode::BAD_REQUEST, "missing kind");
    };
    let entry = entry_for(&state, query_param(&uri, "page_file").as_deref());
    edit_result(crate::edit::schema_descriptor(&entry, &kind))
}

async fn handle_object_kinds(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let entry = entry_for(&state, query_param(&uri, "page_file").as_deref());
    edit_result(crate::edit::object_kinds(&entry))
}

async fn handle_object_instances(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let Some(kind) = query_param(&uri, "kind") else {
        return json_error(StatusCode::BAD_REQUEST, "missing kind");
    };
    let entry = entry_for(&state, query_param(&uri, "page_file").as_deref());
    edit_result(crate::edit::object_instances(&entry, &kind))
}

async fn handle_read_object(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let (Some(kind), Some(file), Some(span)) = (
        query_param(&uri, "kind"),
        query_param(&uri, "file"),
        query_param(&uri, "span"),
    ) else {
        return json_error(StatusCode::BAD_REQUEST, "missing kind/file/span");
    };
    let Some(file) = sandboxed(&state.watch_root, Path::new(&file)) else {
        return json_error(StatusCode::BAD_REQUEST, "file outside the served tree");
    };
    let Some((start, end)) = span.split_once(':') else {
        return json_error(StatusCode::BAD_REQUEST, "bad span");
    };
    let (Ok(start), Ok(end)) = (start.parse(), end.parse()) else {
        return json_error(StatusCode::BAD_REQUEST, "bad span");
    };
    let entry = entry_for(&state, query_param(&uri, "page_file").as_deref());
    edit_result(crate::edit::read_object(
        &entry,
        &file,
        wcl_lang::Span::new(start, end),
        &kind,
    ))
}

async fn handle_object_source(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let (Some(file), Some(span)) = (query_param(&uri, "file"), query_param(&uri, "span")) else {
        return json_error(StatusCode::BAD_REQUEST, "missing file/span");
    };
    let Some(file) = sandboxed(&state.watch_root, Path::new(&file)) else {
        return json_error(StatusCode::BAD_REQUEST, "file outside the served tree");
    };
    let Some((start, end)) = span.split_once(':') else {
        return json_error(StatusCode::BAD_REQUEST, "bad span");
    };
    let (Ok(start), Ok(end)) = (start.parse(), end.parse()) else {
        return json_error(StatusCode::BAD_REQUEST, "bad span");
    };
    edit_result(crate::edit::read_object_source(
        &file,
        wcl_lang::Span::new(start, end),
    ))
}

async fn handle_object_template(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let Some(kind) = query_param(&uri, "kind") else {
        return json_error(StatusCode::BAD_REQUEST, "missing kind");
    };
    let entry = entry_for(&state, query_param(&uri, "page_file").as_deref());
    edit_result(crate::edit::object_template(&entry, &kind))
}

async fn handle_edit_field(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let entry = entry_for(
        &state,
        v.get("page_file").and_then(serde_json::Value::as_str),
    );
    edit_result(crate::edit::field_edit(&state.watch_root, &entry, &v))
}

async fn handle_edit_add(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let entry = entry_for(
        &state,
        v.get("page_file").and_then(serde_json::Value::as_str),
    );
    edit_result(crate::edit::add_block(&state.watch_root, &entry, &v))
}

async fn handle_edit_delete(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let entry = entry_for(
        &state,
        v.get("page_file").and_then(serde_json::Value::as_str),
    );
    edit_result(crate::edit::delete_block(&state.watch_root, &entry, &v))
}

async fn handle_edit_move(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let entry = entry_for(
        &state,
        v.get("page_file").and_then(serde_json::Value::as_str),
    );
    edit_result(crate::edit::move_block(&state.watch_root, &entry, &v))
}

async fn handle_object_post(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let entry = entry_for(
        &state,
        v.get("page_file").and_then(serde_json::Value::as_str),
    );
    edit_result(crate::edit::object_post(&state.watch_root, &entry, &v))
}

async fn handle_answer_js() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        crate::answer::ANSWER_CLIENT_JS,
    )
        .into_response()
}

async fn handle_answers_list(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let entry = entry_for(&state, query_param(&uri, "page_file").as_deref());
    edit_result(crate::answer::answers_list(&entry))
}

async fn handle_answer_post(State(state): State<Arc<ServeState>>, body: String) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let entry = entry_for(
        &state,
        v.get("page_file").and_then(serde_json::Value::as_str),
    );
    edit_result(crate::answer::answer_post(&state.watch_root, &entry, &v))
}

/// Canonicalize `file` and confirm it sits inside `root`, so a comment / edit
/// write can't escape the served source tree. Returns the canonical path to
/// edit.
pub(crate) fn sandboxed(root: &Path, file: &Path) -> Option<PathBuf> {
    let root = std::fs::canonicalize(root).ok()?;
    let file = std::fs::canonicalize(file).ok()?;
    file.starts_with(&root).then_some(file)
}

/// Render a [`comments::CommentRecord`] to a JSON object.
fn comment_to_json(r: &comments::CommentRecord) -> serde_json::Value {
    serde_json::json!({
        "id": r.id,
        "scope": r.scope.as_str(),
        "file": r.file.display().to_string(),
        "page": r.page,
        "page_file": r.page_file,
        "loc": r.loc,
        "target": r.target,
        "quote": r.quote,
        "body": r.body,
        "author": r.author,
        "status": r.status,
    })
}

pub(crate) fn json_response(status: StatusCode, value: &serde_json::Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        value.to_string(),
    )
        .into_response()
}

pub(crate) fn json_error(status: StatusCode, msg: &str) -> Response {
    json_response(status, &serde_json::json!({ "error": msg }))
}

/// Resolve any request path to a file under the output tree and serve
/// it. Handles `/` and directory paths (→ `index.html`), extension-less
/// page names (→ `<name>.html`), and explicit files (`.html`, and the
/// `_wdoc/` assets at any depth). Rejects `..` / backslash components so
/// the dev server can't be walked outside the output directory. While
/// the most recent build failed, HTML requests get the build-failure
/// page instead of stale content; non-HTML assets keep serving the
/// previous build so unrelated tabs don't lose their styling.
async fn handle_static(State(state): State<Arc<ServeState>>, uri: Uri) -> Response {
    let rel = uri.path().trim_start_matches('/');
    if rel.split('/').any(|seg| seg == ".." || seg.contains('\\')) {
        return StatusCode::NOT_FOUND.into_response();
    }
    let path = resolve_path(&state.out, rel);
    let is_html = content_type(&path).starts_with("text/html");
    if is_html {
        let failed = state
            .error
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(err) = failed {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                error_page(&err),
            )
                .into_response();
        }
    }
    match tokio::fs::read(&path).await {
        Ok(mut bytes) => {
            if is_html {
                // Appending after `</html>` is valid enough for a dev
                // server and avoids parsing the page.
                bytes.extend_from_slice(RELOAD_SCRIPT.as_bytes());
                if state.comment_mode {
                    bytes.extend_from_slice(COMMENT_SCRIPT_TAG.as_bytes());
                }
                if state.edit_mode {
                    // The shared source-editor component loads first so the
                    // edit client can instantiate it (plain script tags
                    // execute in document order).
                    bytes.extend_from_slice(crate::edit::EDITOR_SCRIPT_TAG.as_bytes());
                    bytes.extend_from_slice(crate::edit::EDIT_SCRIPT_TAG.as_bytes());
                }
                if state.answer_mode {
                    bytes.extend_from_slice(crate::answer::ANSWER_SCRIPT_TAG.as_bytes());
                }
            }
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, content_type(&path))],
                bytes,
            )
                .into_response()
        }
        Err(e) if e.kind() == ErrorKind::NotFound => (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            format!(
                "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Not found</title></head>\
                 <body><h1>404</h1><p>Nothing at <code>/{rel}</code>.</p>{RELOAD_SCRIPT}</body></html>",
                rel = html_escape(rel)
            ),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("read {}: {e}", path.display()),
        )
            .into_response(),
    }
}

/// Self-contained build-failure page: inline styles only, no assets
/// from the (failed) output tree, plus the reload script so the page
/// replaces itself as soon as a build succeeds.
fn error_page(err: &str) -> String {
    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\"><title>Build failed</title>\
         <style>body{{font-family:system-ui,sans-serif;margin:2rem;background:#1c1c1c;color:#ddd}}\
         h1{{color:#f66}}pre{{background:#111;border-radius:6px;padding:1rem;overflow:auto;\
         white-space:pre-wrap;line-height:1.4}}</style></head>\
         <body><h1>Build failed</h1>\
         <p>The most recent rebuild failed; the page reloads itself once a build succeeds.</p>\
         <pre>{}</pre>{RELOAD_SCRIPT}</body></html>",
        html_escape(err)
    )
}

/// Minimal HTML escaping for text dropped into the error/404 pages.
fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Map a request-relative path to a file in the output tree: `/` and
/// directories resolve to their `index.html`, an extension-less name to
/// `<name>.html` (else a directory index), and an explicit file as-is.
fn resolve_path(out: &Path, rel: &str) -> PathBuf {
    if rel.is_empty() {
        return out.join("index.html");
    }
    let candidate = out.join(rel);
    if candidate.is_dir() {
        return candidate.join("index.html");
    }
    if candidate.extension().is_some() {
        return candidate;
    }
    let as_html = out.join(format!("{rel}.html"));
    if as_html.exists() {
        return as_html;
    }
    let dir_index = candidate.join("index.html");
    if dir_index.exists() {
        return dir_index;
    }
    as_html
}

/// Map an output file's extension to a content type.
fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        _ => "application/octet-stream",
    }
}

async fn log_requests(req: axum::http::Request<Body>, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let response = next.run(req).await;
    println!("{} {} {}", method, path, response.status().as_u16());
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_escape_escapes_markup() {
        assert_eq!(
            html_escape("a < b && c > d"),
            "a &lt; b &amp;&amp; c &gt; d"
        );
        assert_eq!(html_escape("plain"), "plain");
    }

    #[test]
    fn error_page_embeds_escaped_error_and_reload_script() {
        let page = error_page("expected `<value>`");
        assert!(page.contains("expected `&lt;value&gt;`"));
        assert!(!page.contains("expected `<value>`"));
        assert!(page.contains("__wdoc_reload"));
    }

    #[test]
    fn reload_script_targets_the_reload_route() {
        assert!(RELOAD_SCRIPT.contains("/__wdoc_reload"));
        assert!(RELOAD_SCRIPT.contains("location.reload()"));
    }
}
