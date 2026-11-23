<script lang="ts">
 import {onMount} from "svelte";
 import {invoke} from "@tauri-apps/api/core";
 import {language} from "$lib/language.svelte";
 import {makeBirdReader,birdHref,groupBirdVisits,type BirdList} from "$lib/birds";
 import {formatBirdDate} from "$lib/bird-names";
 import BirdVisitPreview from "$lib/components/BirdVisitPreview.svelte";
 import * as m from "$lib/paraglide/messages.js";
 let ebirdPreviewAvailable=$state(false);
 let available=$state<boolean|null>(null),lists=$state<BirdList[]>([]),failed=$state(false),loading=$state(true);
 const groups=$derived(groupBirdVisits(lists));
 onMount(()=>{
  let disposed=false,revision=0;
  async function load(){
   const request=++revision;loading=true;failed=false;
   try{
    const enabled=await invoke<boolean>("bird_observations_available");if(disposed||request!==revision)return;available=enabled;if(!enabled){ebirdPreviewAvailable=false;return;}
    ebirdPreviewAvailable=await invoke<boolean>("ebird_preview_available");if(disposed||request!==revision)return;
    const session=await invoke<{generation:number}>("new_journal_session");
    const result=await makeBirdReader()<BirdList[]>("read_bird_lists",{generation:session.generation});
    if(!disposed&&request===revision)lists=result.data;
   }catch{if(!disposed&&request===revision)failed=true;}
   finally{if(!disposed&&request===revision)loading=false;}
  }
  const refresh=()=>void load();window.addEventListener("mno:journal-changed",refresh);void load();
  return()=>{disposed=true;window.removeEventListener("mno:journal-changed",refresh);};
 });
</script>
<svelte:head><title>{m.bird_overview({}, {locale:language.current})} — MNO</title></svelte:head>
<main>
 <h1>{m.bird_overview({}, {locale:language.current})}</h1>
 {#if available===false}<p>{m.bird_unavailable({}, {locale:language.current})}</p>
 {:else if failed}<p role="alert">{m.bird_load_failed({}, {locale:language.current})}</p>
 {:else if loading}<p role="status">{m.bird_loading({}, {locale:language.current})}</p>
 {:else if available}
  <p class="intro">{m.bird_overview_intro({}, {locale:language.current})}</p>
  <div class="observation-actions">
  <a class="create" href="/observations/new">＋ {m.bird_create({}, {locale:language.current})}</a>
  {#if ebirdPreviewAvailable}<a class="create" href="/observations/ebird-preview">{m.ebird_title({}, {locale:language.current})}</a>{/if}
  </div>
  {#if !lists.length}<p>{m.bird_no_lists({}, {locale:language.current})}</p>{/if}
  {#each groups as group (group.date)}<section class="date-group" aria-label={formatBirdDate(group.date,language.current)}>
   <h2><time datetime={group.date}>{formatBirdDate(group.date,language.current)}</time></h2>
   <ol>{#each group.visits as list (list.id)}<li><BirdVisitPreview {list} journalDate={group.date} locale={language.current} href={birdHref(null,list.id)}/></li>{/each}</ol>
  </section>{/each}
 {/if}
</main>
<style>
 main{box-sizing:border-box;max-width:824px;margin:0 auto;padding:24px 32px 60px;color:#343530;font:17px/1.5 Georgia,serif}h1{font:normal 28px/1.3 Georgia,serif;margin:0 0 14px}.intro{color:#686d63;margin:0 0 12px}.observation-actions{display:flex;align-items:center;flex-wrap:wrap;gap:0 24px}.create{display:inline-flex;align-items:center;min-height:44px;color:#435b47;font:13px system-ui,sans-serif}.date-group{margin-top:26px}h2{font:normal 21px/1.4 Georgia,serif;color:#435b47;margin:0 0 8px}ol{list-style:none;padding:0;margin:0}li{border-bottom:1px solid #e2e3dc;padding:5px 0}a:focus-visible{outline:2px solid #788e7b;outline-offset:3px}@media(max-width:600px){main{padding:20px 24px 40px}}
</style>
