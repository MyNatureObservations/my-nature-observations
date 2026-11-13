<script lang="ts">
 import {validTime} from "$lib/birds";
 import * as m from "$lib/paraglide/messages.js";
 let {value=$bindable(""),id,label,locale,disabled=false,descriptionId=undefined}:{value:string;id:string;label:string;locale:"sv"|"en";disabled?:boolean;descriptionId?:string}=$props();
 let open=$state(false);let picker:HTMLDetailsElement;let input:HTMLInputElement|undefined=$state(undefined);
 const hours=Array.from({length:24},(_,i)=>String(i).padStart(2,"0"));
 const minutes=Array.from({length:60},(_,i)=>String(i).padStart(2,"0"));
 const hour=$derived(/^([01]\d|2[0-3]):[0-5]\d$/.test(value)?value.slice(0,2):"");
 const minute=$derived(/^([01]\d|2[0-3]):[0-5]\d$/.test(value)?value.slice(3,5):"");
 $effect(()=>{if(disabled)open=false;});
 function selectPart(part:"hour"|"minute",selected:string){
  if(disabled||!(part==="hour"?hours:minutes).includes(selected))return;
  const match=/^([01]\d|2[0-3]):([0-5]\d)$/.exec(value);
  value=part==="hour"?`${selected}:${match?.[2]??"00"}`:`${match?.[1]??"00"}:${selected}`;
 }
 function typeTime(event:Event){if(!disabled)value=(event.currentTarget as HTMLInputElement).value;}
 function clear(){if(disabled)return;value="";open=false;input?.focus();}
 function keydown(event:KeyboardEvent){
  if(open&&event.key==="Escape"&&event.target instanceof Node&&picker?.parentElement?.contains(event.target)){
   event.preventDefault();event.stopPropagation();open=false;picker.querySelector("summary")?.focus();
  }
 }
 function leaveControl(event:FocusEvent){if(event.relatedTarget instanceof Node&&!(event.currentTarget as HTMLElement).contains(event.relatedTarget))open=false;}
 function outside(event:PointerEvent){if(open&&event.target instanceof Node&&!picker?.parentElement?.contains(event.target))open=false;}
</script>
<svelte:window onkeydown={keydown} onpointerdown={outside}/>
<div class="time-control" onfocusout={leaveControl}>
 <label for={id}>{label}</label>
 <div class="time-picker" class:invalid={!validTime(value)}>
  <input {id} type="text" inputmode="numeric" placeholder="--:--" maxlength="5" bind:this={input} value={value} oninput={typeTime} {disabled} aria-invalid={!validTime(value)} aria-describedby={descriptionId} autocomplete="off"/>
  <details bind:this={picker} bind:open>
   <summary aria-label={m.bird_choose_time({time:label},{locale})} aria-disabled={disabled} tabindex={disabled?-1:0} onclick={event=>{if(disabled)event.preventDefault();}}>
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>
   </summary>
   <div class="time-panel">
    <div class="columns">
     <label><span>{m.bird_hours({}, {locale})}</span><select size="5" value={hour} {disabled} aria-label={`${label}: ${m.bird_hours({}, {locale})}`} onchange={e=>selectPart("hour",e.currentTarget.value)}><option value="" disabled hidden>--</option>{#each hours as h}<option value={h}>{h}</option>{/each}</select></label>
     <label><span>{m.bird_minutes({}, {locale})}</span><select size="5" value={minute} {disabled} aria-label={`${label}: ${m.bird_minutes({}, {locale})}`} onchange={e=>selectPart("minute",e.currentTarget.value)}><option value="" disabled hidden>--</option>{#each minutes as minute}<option value={minute}>{minute}</option>{/each}</select></label>
    </div>
    <button type="button" disabled={disabled||!value} onclick={clear}>{m.bird_clear_time({}, {locale})}</button>
   </div>
  </details>
 </div>
</div>
<style>
 .time-control{display:inline-flex;align-items:center;gap:8px;max-width:100%;color:var(--text-muted,#686d63);font:12px system-ui,sans-serif}
 .time-picker{position:relative;display:inline-flex;align-items:center;border:1px solid #cccec7;border-radius:6px;background:#fffefa}
 .time-picker.invalid{border-color:#b4756f}
 input,select{box-sizing:border-box;color:#343530;font:13px system-ui,sans-serif;font-variant-numeric:tabular-nums}
 input{width:69px;padding:7px 8px;min-height:36px;border:0;background:transparent;border-radius:6px}
 details{position:static}
 summary{box-sizing:border-box;display:grid;place-items:center;width:32px;min-height:36px;border-radius:5px;color:var(--text-muted,#686d63);cursor:pointer;list-style:none}
 summary::-webkit-details-marker{display:none}summary:hover{color:var(--accent,#435b47);background:#eef1eb}
 summary[aria-disabled="true"],button:disabled{opacity:.5;cursor:default}
 .time-panel{position:absolute;z-index:3;top:calc(100% + 5px);left:0;width:172px;box-sizing:border-box;padding:12px;border:1px solid #d9d8d1;border-radius:8px;background:var(--surface,#faf9f6);color:var(--text-muted,#686d63);font:11px/1.4 system-ui,sans-serif}
 .columns{display:grid;grid-template-columns:1fr 1fr;gap:10px}.columns label{display:grid;gap:6px}
 select{width:100%;height:126px;overflow-y:auto;border:0;border-radius:4px;background:#f3f3ed;text-align:center;padding:2px}
 option{padding:5px 8px;border-radius:3px}option:checked{background:#e1e8dd;color:#354f3b}
 button{display:block;width:100%;min-height:36px;margin-top:9px;border:0;border-top:1px solid #e2e3dc;padding:8px 0 0;background:transparent;color:var(--accent,#435b47);font:12px system-ui,sans-serif;text-align:left;cursor:pointer}
 :focus-visible{outline:2px solid #788e7b;outline-offset:3px}
</style>
