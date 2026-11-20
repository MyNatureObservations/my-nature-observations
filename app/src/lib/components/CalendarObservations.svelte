<script lang="ts">
 import {createObservationCalendarReader,calendarVisits} from '$lib/observation-calendar';
 import {birdHref,type BirdList} from '$lib/birds';
 import BirdVisitPreview from '$lib/components/BirdVisitPreview.svelte';
 import {language} from '$lib/language.svelte';
 import * as m from '$lib/paraglide/messages.js';
 let {date}:{date:string}=$props();
 let visits=$state<BirdList[]>([]),available=$state(false),loading=$state(true),failed=$state(false);
 const read=createObservationCalendarReader();let request=0,disposed=false;
 async function load(selected:string){
  const ticket=++request;visits=[];loading=true;failed=false;
  try{const result=await read();if(disposed||ticket!==request||selected!==date)return;available=result!==null;visits=result?calendarVisits(result.data,selected):[];}
  catch{if(!disposed&&ticket===request)failed=true;}
  finally{if(!disposed&&ticket===request)loading=false;}
 }
 $effect(()=>{void load(date);});
 $effect(()=>{const refresh=()=>void load(date);window.addEventListener('mno:journal-changed',refresh);return()=>{disposed=true;request++;window.removeEventListener('mno:journal-changed',refresh);};});
</script>
{#if available||failed}
 <section aria-label={m.observation_home({}, {locale:language.current})}>
  <h3>{m.observation_home({}, {locale:language.current})}</h3>
  {#if failed}<p role="alert">{m.observation_calendar_error({}, {locale:language.current})}</p>
  {:else if loading}<p role="status">{m.bird_loading({}, {locale:language.current})}</p>
  {:else if !visits.length}<p>{m.observation_day_empty({}, {locale:language.current})}</p>
  {:else}<ul>{#each visits as list (list.id)}<li><BirdVisitPreview {list} journalDate={date} locale={language.current} href={birdHref(null,list.id)}/></li>{/each}</ul>{/if}
  {#if available}<a href="/observations">{m.observation_home({}, {locale:language.current})} →</a>{/if}
 </section>
{/if}
<style>
 section{margin-top:24px}h3,a{font:13px/1.5 system-ui,sans-serif;color:var(--accent,#435b47)}h3{font-weight:500;margin:0 0 8px}ul{list-style:none;padding:0;margin:0}li{border-bottom:1px solid #d9d8d1}p{font:13px/1.7 system-ui,sans-serif;color:var(--text-muted,#686d63)}a{display:inline-flex;align-items:center;min-height:40px}a:focus-visible{outline:2px solid #788e7b;outline-offset:3px}
</style>
