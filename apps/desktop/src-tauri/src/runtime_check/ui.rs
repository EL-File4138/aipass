//! Read-only geometry diagnostics for the already isolated interactive fixture.
use aipass_storage::atomic_write_bytes;
use std::{path::PathBuf, time::Duration};
use tauri::Manager;

pub(super) fn start(app: tauri::AppHandle, root: PathBuf) {
    std::thread::spawn(move || loop {
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        let native = serde_json::json!({
            "innerSize": window.inner_size().ok(), "outerSize": window.outer_size().ok(),
            "position": window.inner_position().ok(), "maximized": window.is_maximized().ok(),
            "minimized": window.is_minimized().ok(), "visible": window.is_visible().ok(),
            "scale": window.scale_factor().ok(), "version": app.package_info().version.to_string(),
        });
        let _ = atomic_write_bytes(
            root.join("window.json"),
            &serde_json::to_vec(&native).unwrap(),
        );
        let path = root.join("ui.json");
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let _ = window.eval_with_callback(
            r#"JSON.stringify((() => {
                const visible = e => { const r=e.getBoundingClientRect(); const s=getComputedStyle(e);
                    if (!(r.width>0 && r.height>0 && s.visibility!=='hidden' && s.display!=='none' && r.bottom>0 && r.right>0 && r.top<innerHeight && r.left<innerWidth)) return false;
                    const hit=document.elementFromPoint(Math.max(0,Math.min(innerWidth-1,r.x+r.width/2)),Math.max(0,Math.min(innerHeight-1,r.y+r.height/2)));
                    return !!hit && (e===hit || e.contains(hit)); };
                const rect = e => { const r=e.getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom}; };
                const control = e => { const s=getComputedStyle(e); return {
                    type:e.getAttribute('type'), disabled:e.matches(':disabled,[data-disabled]'),
                    readOnly:!!e.readOnly, required:!!e.required, checked:e.checked ?? e.getAttribute('data-state'),
                    focused:e===document.activeElement, focusVisible:e.matches(':focus-visible'),
                    valid:e.validity?.valid, invalid:e.getAttribute('aria-invalid'),
                    color:s.color, background:s.backgroundColor, border:s.borderColor,
                    outline:s.outline, shadow:s.boxShadow, opacity:s.opacity,
                    colorScheme:s.colorScheme, accentColor:s.accentColor, textFill:s.webkitTextFillColor,
                }; };
                const selector = '.workspace,.sidebar,.list-pane,.toolbar,.detail,.detail-header,.identity,.identity-text,.provider-icon,.meta,.actions,.agent-management,.win-controls,.items-list-slot,[role=dialog],[role=menu],[role=menuitem],[role=option],[role=tab],button,input,textarea,select,h1,h2,h3,label';
                return {time:Date.now(),viewport:{width:innerWidth,height:innerHeight},platform:document.documentElement.dataset,
                    elements:[...document.querySelectorAll(selector)].filter(visible).map(e=>({tag:e.tagName,role:e.getAttribute('role'),label:e.getAttribute('aria-label')||e.getAttribute('placeholder')||e.getAttribute('title')||(e.matches('button,h1,h2,h3,label,[role=tab],[role=menuitem],[role=option]')?e.innerText.trim().slice(0,160):''),classes:typeof e.className==='string'?e.className:'',rect:rect(e),disabled:!!e.disabled,control:control(e),scrollHeight:e.scrollHeight,clientHeight:e.clientHeight,overflowY:getComputedStyle(e).overflowY})),
                    overflow:[...document.querySelectorAll('#app *')].filter(e=>visible(e)&&!e.matches('input,textarea')&&e.clientWidth>0&&e.scrollWidth>e.clientWidth+2&&getComputedStyle(e).textOverflow!=='ellipsis'&&getComputedStyle(e).overflowX==='visible').slice(0,30).map(e=>({tag:e.tagName,classes:e.className,rect:rect(e),scrollWidth:e.scrollWidth,clientWidth:e.clientWidth}))};
            })())"#,
            move |result| {
                let _ = atomic_write_bytes(&path, result.as_bytes());
                let _ = send.send(());
            },
        );
        let _ = receive.recv_timeout(Duration::from_secs(2));
        std::thread::sleep(Duration::from_millis(500));
    });
}
