use std::future::IntoFuture;
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
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
use wcl_lang::Span;
use wcl_wdoc::{BuildOptions, build_with_options, comments};

/// How long the watch loop waits for the event stream to go quiet
/// before rebuilding — one editor save fires several notify events,
/// which should coalesce into a single build.
const QUIET_WINDOW: Duration = Duration::from_millis(150);

/// How long a live-reload long-poll request parks before answering
/// with the unchanged generation. Short enough that intermediaries
/// don't kill the connection; the client just re-polls.
const POLL_TIMEOUT: Duration = Duration::from_secs(30);

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
.wcl-bar button{background:#4c8bf5;color:#fff;border:0;border-radius:20px;padding:9px 16px;
 font:600 13px system-ui;cursor:pointer;box-shadow:0 6px 20px rgba(0,0,0,.4)}
.wcl-bar button.on{background:#e0a000;color:#1c1c1c}
.wcl-bar button.wcl-count{background:#2f6f4f}
.wcl-hint{position:fixed;top:0;left:0;right:0;z-index:99999;background:#4c8bf5;color:#fff;
 text-align:center;padding:7px;font:600 13px system-ui}
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
`;
const st=document.createElement('style');st.textContent=CSS;document.head.appendChild(st);

const pageEl=document.querySelector('[data-wcl-page-file]');
function esc(s){return (s||'').replace(/[&<>]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;'}[c]));}
function chrome(t){return t.closest('.wcl-pop')||t.closest('.wcl-bar')||t.closest('.wcl-hint')||t.closest('.wcl-pin');}

// Positional locator: index path among [data-wcl-block] from the page root.
function locOf(el){
 const path=[];let n=el;
 while(n && n!==pageEl){
  if(n.hasAttribute&&n.hasAttribute('data-wcl-block')){
   const sibs=[...n.parentNode.children].filter(c=>c.hasAttribute&&c.hasAttribute('data-wcl-block'));
   path.unshift(sibs.indexOf(n));
  }
  n=n.parentNode;
 }
 return path.join('/');
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
async function post(payload,p){
 let res;
 try{res=await fetch('/__wdoc_comment',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(payload)});}
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
  const body=ta.value.trim();if(!body)return;
  const anchorable=el.hasAttribute('data-wcl-file');
  let payload={body,quote:quote||null};
  if(onPage||!anchorable){
   if(!pageEl)return;
   const ps=pageEl.getAttribute('data-wcl-page-span').split('..');
   payload.on_page=true;
   payload.file=pageEl.getAttribute('data-wcl-page-file');
   payload.span_start=+ps[0];payload.span_end=+ps[1];
   if(!onPage){payload.loc=locOf(el);payload.target=descOf(el);}
  }else{
   const s=el.getAttribute('data-wcl-span').split('..');
   payload.on_page=false;
   payload.file=el.getAttribute('data-wcl-file');
   payload.span_start=+s[0];payload.span_end=+s[1];
  }
  if(await post(payload,pop))closePop(); // a rebuild + live-reload re-renders with the comment shown
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

// Existing comments: pin + click to view / resolve.
for(const el of document.querySelectorAll('[data-wcl-comment-id]')){
 const id=el.getAttribute('data-wcl-comment-id');
 const body=el.getAttribute('data-wcl-comment')||'';
 const pin=document.createElement('div');pin.className='wcl-pin';pin.textContent='✓';pin.title=body;
 pin.onclick=ev=>{ev.stopPropagation();ev.preventDefault();showComment(el,id,body);};
 el.appendChild(pin);
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
  if(await post({resolve_id:id},pop))closePop();
 };
}

// Comments on *this* page: fetched from the server and filtered to the page's
// own source file + name, so it covers all three shapes (direct, page-attached,
// generic) — not just the inline pins.
let allComments=[];
function pageComments(){
 if(!pageEl)return [];
 const pf=pageEl.getAttribute('data-wcl-page-file'),pn=pageEl.getAttribute('data-wcl-page-name');
 return allComments.filter(c=>c.file===pf&&c.page===pn);
}
async function refresh(){
 try{const r=await fetch('/__wdoc_comment');allComments=r.ok?await r.json():[];}catch(_){allComments=[];}
 const n=pageComments().length;
 countBtn.textContent='💬 '+n+(n===1?' comment':' comments');
}

// Jump-to works for comments with a visible inline anchor (direct block
// comments, which carry a data-wcl-comment-id pin).
function elForComment(c){return document.querySelector('[data-wcl-comment-id="'+c.id+'"]');}
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
   const where=c.scope==='page'?'Whole page':(c.target||c.host_kind||'block');
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
const countBtn=document.createElement('button');countBtn.className='wcl-count';countBtn.textContent='💬 …';
countBtn.onclick=openModal;
const selBtn=document.createElement('button');selBtn.textContent='🎯 Comment on a block';
selBtn.onclick=()=>setPick(!picking);
const pageBtn=document.createElement('button');pageBtn.textContent='💬 Comment on page';
pageBtn.onclick=()=>{setPick(false);if(pageEl)openForm(pageEl,true);};
if(pageEl)bar.appendChild(countBtn);
bar.appendChild(selBtn);if(pageEl)bar.appendChild(pageBtn);
document.body.appendChild(bar);
refresh();
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
    /// The document the dev server is serving, scanned by the comment-list
    /// endpoint.
    src_file: PathBuf,
    /// Watched source root; comment writes are sandboxed to within it.
    watch_root: PathBuf,
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

pub(crate) async fn serve(
    file: PathBuf,
    out: Option<PathBuf>,
    addr: SocketAddr,
    site: Option<String>,
    comment_mode: bool,
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
    let temp_cleanup = _tempdir_guard.as_ref().map(|td| td.path().to_path_buf());
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nshutting down");
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

    let state = Arc::new(ServeState {
        out: out_dir.clone(),
        error: RwLock::new(None),
        generation: tokio::sync::watch::Sender::new(0),
        comment_mode,
        src_file: file.clone(),
        watch_root: watch_root.clone(),
    });

    // Initial build. Failure is non-fatal — the watcher will retry, and
    // HTML requests serve the build-failure page in the meantime.
    run_build(&file, &out_dir, site.as_deref(), &state, false);

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
        if let Ok(event) = res {
            let _ = tx.send(event);
        }
    })?;
    watcher.watch(&watch_root, RecursiveMode::Recursive)?;

    let bg_file = file.clone();
    let bg_out = out_dir.clone();
    let bg_site = site.clone();
    let bg_state = Arc::clone(&state);
    // The rebuild loop is a *local* future (not a detached `tokio::spawn`)
    // owned by the `select!` below, so when the server shuts down it is
    // dropped here — which drops the `notify` watcher and stops its inotify
    // thread. A detached task would outlive `serve` and hang process exit.
    let watch_loop = async move {
        let _watcher = watcher; // keep the watcher alive for this future's lifetime
        while let Some(event) = rx.recv().await {
            if !is_relevant(&event) {
                continue;
            }
            drain_quiet(&mut rx).await;
            run_build(&bg_file, &bg_out, bg_site.as_deref(), &bg_state, true);
        }
    };

    // One generic static handler resolves any request path against the
    // output tree, so it serves both the flat single-site layout and the
    // nested multi-site one (`/<site>/…`, `/<site>/_wdoc/…`) plus the
    // generated chooser at `/`, with no per-route knowledge. The reload
    // endpoint sits outside the output tree's namespace.
    let mut app = Router::new().route("/__wdoc_reload", get(handle_reload));
    if comment_mode {
        app = app
            .route("/__wdoc_comment.js", get(handle_comment_js))
            .route(
                "/__wdoc_comment",
                get(handle_comment_list).post(handle_comment_post),
            );
    }
    let app = app
        .fallback(get(handle_static))
        .with_state(Arc::clone(&state))
        .layer(middleware::from_fn(log_requests));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    let bound = listener.local_addr()?;
    println!(
        "serving http://{bound}  (source: {}, out: {})",
        file.display(),
        out_dir.display()
    );

    // Run the server and the watch loop concurrently. Neither branch
    // completes in normal operation — the loop runs until the Ctrl-C task
    // above hard-exits the process. No graceful shutdown: a parked reload
    // long-poll (up to `POLL_TIMEOUT`) must not delay teardown.
    tokio::select! {
        res = axum::serve(listener, app).into_future() => res?,
        _ = watch_loop => {}
    }
    Ok(())
}

/// Keep draining the event channel until `QUIET_WINDOW` passes with no
/// further *relevant* event. Irrelevant events (e.g. the build's own
/// output writes when `--out` sits inside the watched tree) are
/// swallowed without extending the window.
async fn drain_quiet(rx: &mut UnboundedReceiver<Event>) {
    let mut deadline = tokio::time::Instant::now() + QUIET_WINDOW;
    loop {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(ev)) if is_relevant(&ev) => {
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
    ) && event
        .paths
        .iter()
        .any(|p| p.extension().is_some_and(|e| e == "wcl"))
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
    match comments::list(&state.src_file, None) {
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
        return match comments::resolve(&state.src_file, None, id) {
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
        return match comments::edit(&state.src_file, None, id, body_text) {
            Ok(true) => json_response(StatusCode::OK, &serde_json::json!({ "edited": id })),
            Ok(false) => json_error(StatusCode::NOT_FOUND, "no such comment"),
            Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.render_plain()),
        };
    }
    let Some(file) = str_of("file") else {
        return json_error(StatusCode::BAD_REQUEST, "missing file");
    };
    let (Some(s), Some(e)) = (
        v.get("span_start").and_then(serde_json::Value::as_u64),
        v.get("span_end").and_then(serde_json::Value::as_u64),
    ) else {
        return json_error(StatusCode::BAD_REQUEST, "missing span");
    };
    let Some(path) = sandboxed(&state.watch_root, Path::new(file)) else {
        return json_error(StatusCode::FORBIDDEN, "file outside the served root");
    };
    let span = Span::new(s as usize, e as usize);
    let author = str_of("author");
    let quote = str_of("quote");
    let on_page = v
        .get("on_page")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let res = if on_page {
        comments::add_to_page(
            &path,
            span,
            body_text,
            author,
            str_of("loc"),
            str_of("target"),
            quote,
        )
    } else {
        comments::add_to_block(&path, span, body_text, author, quote)
    };
    match res {
        Ok(id) => json_response(StatusCode::OK, &serde_json::json!({ "id": id })),
        Err(e) => json_error(StatusCode::INTERNAL_SERVER_ERROR, &e.render_plain()),
    }
}

/// Canonicalize `file` and confirm it sits inside `root`, so a comment write
/// can't escape the served source tree. Returns the canonical path to edit.
fn sandboxed(root: &Path, file: &Path) -> Option<PathBuf> {
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
        "host_kind": r.host_kind,
        "host_label": r.host_label,
        "loc": r.loc,
        "target": r.target,
        "quote": r.quote,
        "body": r.body,
        "author": r.author,
        "status": r.status,
        "span_start": r.span_start,
        "span_end": r.span_end,
    })
}

fn json_response(status: StatusCode, value: &serde_json::Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        value.to_string(),
    )
        .into_response()
}

fn json_error(status: StatusCode, msg: &str) -> Response {
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
