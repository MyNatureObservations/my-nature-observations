<script lang="ts">
 import {onMount} from "svelte";
 import {invoke} from "@tauri-apps/api/core";
 import {goto} from "$app/navigation";
 import {page} from "$app/state";
 import {language} from "$lib/language.svelte";
 import {approveBirdDisable,setBirdModule,birdModule,refreshBirdModuleViews} from "$lib/bird-module.svelte";
 import * as m from "$lib/paraglide/messages.js";
 let birdHelpOpen=$state(false),ebirdHelpOpen=$state(false);
 let ebirdEnabled=$state(false),enabled=$state(false),supported=$state(false),loaded=$state(false),error=$state("");
 onMount(()=>{let disposed=false;void Promise.all([invoke<boolean>("bird_module_supported"),invoke<boolean>("bird_module_enabled"),invoke<boolean>("ebird_connection_enabled")]).then(([s,e,connection])=>{if(!disposed){supported=s;enabled=e;ebirdEnabled=connection;loaded=true;}}).catch(()=>{if(!disposed)error="failed";});return()=>{disposed=true;};});
 async function changeConnection(){
  if(!enabled||!loaded||birdModule.changing)return;
  const next=!ebirdEnabled;
  error="";
  birdModule.changing=true;
  try{
   if(!next&&!await approveBirdDisable()){error="busy";return;}
   if(!next&&page.url.pathname==="/observations/ebird-preview")await goto("/observations");
   await invoke("set_ebird_connection_enabled",{enabled:next});
   ebirdEnabled=next;
   refreshBirdModuleViews();
  }catch{
   error="failed";
   try{ebirdEnabled=await invoke<boolean>("ebird_connection_enabled");refreshBirdModuleViews();}
   catch{loaded=false;}
  }finally{birdModule.changing=false;}
 }
 async function change(){
  if(birdModule.changing||!loaded)return;
  const next=!enabled;error="";birdModule.changing=true;
  try{
   if(!next){
    if(!await approveBirdDisable()){error="busy";return;}
    const match=page.url.pathname.match(/^\/journal\/entries\/([^/]+)\/birds(?:\/|$)/);
    // Navigation runs existing leave-page safeguards before committing the choice.
    if(match||page.url.pathname.startsWith("/observations")){
     let destination="/";
     if(match)try{const session=await invoke<{generation:number}>("new_journal_session");const response=await invoke<{data:{id:string}|null}>("get_journal_entry",{id:decodeURIComponent(match[1]),generation:session.generation});if(response.data)destination=`/journal/entries/${match[1]}`;}catch{}
     await goto(destination);
    }
   }
   if(next&&(page.url.pathname.startsWith("/observations")||/\/birds(?:\/|$)/.test(page.url.pathname)))await goto("/");
   await setBirdModule(next);enabled=next;
  }catch{error="failed";try{const actual=await invoke<boolean>("bird_module_enabled");if(actual!==enabled){enabled=actual;refreshBirdModuleViews();}}catch{loaded=false;}}finally{birdModule.changing=false;}
 }
</script>
{#if supported||error}
<section class="module-setting" aria-labelledby="modules-heading">
 <details class="module-details">
 <summary id="modules-heading" class="panel-section-heading">
  <svg xmlns="http://www.w3.org/2000/svg" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>
  <span>{m.modules_heading({}, {locale:language.current})}</span>
  <span class="language-chevron" aria-hidden="true">⌄</span>
 </summary>
 <div class="setting-row">
  <label for="bird-module-switch">{m.modules_birds({}, {locale:language.current})}</label>
  <button type="button" class="setting-help" aria-label={m.modules_birds_help({}, {locale:language.current})} aria-expanded={birdHelpOpen} aria-controls="bird-module-description" onclick={()=>birdHelpOpen=!birdHelpOpen}><span aria-hidden="true">?</span></button>
  <p id="bird-module-description" class="help-text" hidden={!birdHelpOpen}>{m.modules_birds_explanation({}, {locale:language.current})}</p>
  <input id="bird-module-switch" type="checkbox" role="switch" checked={enabled} disabled={!loaded||birdModule.changing} onchange={(event)=>{event.currentTarget.checked=enabled;void change();}}/>
 </div>
 <div class="source-setting setting-row">
  <label for="ebird-connection-switch">{m.modules_ebird({}, {locale:language.current})}</label>
  <button type="button" class="setting-help" aria-label={m.modules_ebird_help({}, {locale:language.current})} aria-expanded={ebirdHelpOpen} aria-controls="ebird-connection-description" onclick={()=>ebirdHelpOpen=!ebirdHelpOpen}><span aria-hidden="true">?</span></button>
  <p id="ebird-connection-description" class="help-text" hidden={!ebirdHelpOpen}>{m.modules_ebird_explanation({}, {locale:language.current})}</p>
  <input id="ebird-connection-switch" type="checkbox" role="switch" aria-describedby={!enabled?"ebird-connection-dependency":undefined} checked={ebirdEnabled} disabled={!enabled||!loaded||birdModule.changing} onchange={(event)=>{event.currentTarget.checked=ebirdEnabled;void changeConnection();}}/>
  {#if !enabled}<p id="ebird-connection-dependency">{m.modules_ebird_dependency({}, {locale:language.current})}</p>{/if}
 </div>
 {#if error}<p role="alert">{error==="busy"?m.modules_pending({}, {locale:language.current}):m.modules_failed({}, {locale:language.current})}</p>{/if}
</details>
</section>
{/if}
<style>
 .module-setting {margin-top:28px;font-family:-apple-system,BlinkMacSystemFont,sans-serif;color:var(--text,#343530);}
 .panel-section-heading {display:flex;align-items:center;gap:12px;margin-bottom:0;padding:8px 0;font-family:-apple-system,BlinkMacSystemFont,sans-serif;font-size:13px;font-weight:500;color:var(--accent,#435b47);cursor:pointer;list-style:none;}
 .panel-section-heading::-webkit-details-marker {display:none;}
 .panel-section-heading:focus-visible {outline:2px solid #788e7b;outline-offset:3px;border-radius:3px;}
 .panel-section-heading svg {flex-shrink:0;}
 .language-chevron {margin-left:auto;font-size:16px;transition:transform 150ms ease;}
 .module-details[open] > summary .language-chevron {transform:rotate(180deg);}
 @media (prefers-reduced-motion:reduce) {.language-chevron{transition:none;}}
 .setting-row {display:grid;grid-template-columns:auto 1fr auto;align-items:start;column-gap:6px;}
 .setting-row input {margin-top:11px;}
 .setting-help {grid-column:2;grid-row:1;border:0;padding:0;background:transparent;display:grid;place-items:center;width:28px;height:44px;list-style:none;color:var(--accent,#435b47);cursor:pointer;}
 .setting-help span {display:grid;place-items:center;width:16px;height:16px;border:1px solid currentColor;border-radius:50%;font-size:11px;}
 .setting-help:focus-visible {outline:2px solid var(--accent,#435b47);outline-offset:2px;border-radius:4px;}
 .setting-row > p {grid-column:1/-1;}
 .setting-row .help-text {grid-row:2;}
 #ebird-connection-dependency {grid-row:3;}
 .setting-row input {grid-column:3;grid-row:1;}
 .source-setting {margin:8px 0 0 16px;}
 label {display:flex;align-items:center;min-height:44px;font-size:13px;font-weight:500;line-height:1.5;cursor:pointer;}
 p {margin:2px 0 0;font-size:12px;line-height:1.6;color:var(--text-muted,#686d63);}
 input {appearance:none;-webkit-appearance:none;position:relative;flex:0 0 auto;box-sizing:border-box;width:36px;height:22px;margin:0;border:1px solid #a7ada3;border-radius:12px;background:#e3e5dd;cursor:pointer;}
 input::before {content:"";position:absolute;top:2px;left:2px;width:16px;height:16px;border-radius:50%;background:#f8f7f3;}
 input:checked {background:var(--accent,#435b47);border-color:var(--accent,#435b47);}
 input:checked::before {left:16px;}
 input:focus-visible {outline:2px solid var(--accent,#435b47);outline-offset:3px;}
 input:disabled {opacity:.5;cursor:default;}
 [role="alert"] {color:var(--error,#9a403b);}
 @media (forced-colors:active) {input{appearance:auto;-webkit-appearance:auto;}input::before{display:none;}}
</style>
