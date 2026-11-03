<script lang="ts">
  import WeatherIcon from "$lib/components/WeatherIcon.svelte";
  import WeatherOverview from "$lib/components/WeatherOverview.svelte";
  import WeatherForm from "$lib/components/WeatherForm.svelte";
  import HistoricalWeather from "$lib/components/HistoricalWeather.svelte";
  import type { WeatherPart } from "$lib/weather";
  import WeatherDetails from "$lib/components/WeatherDetails.svelte";
  import { weatherPanelPosition } from "$lib/weather-panel";
  import { tick, untrack } from "svelte";
  import type { WeatherStatus } from "$lib/weather";
  import { weatherLabels } from "$lib/weather-labels";
  let { status, locale, editing = false, date = "", available = false, showEmpty = false, entryId = null, generation = null, contextPending = false, historicalSaved = async () => {}, onDraft = (_d:boolean,_b:boolean) => {}, saveManual = async (_part:WeatherPart|null) => {}, reload = async () => {} }: { status: WeatherStatus; locale: "sv" | "en"; editing?: boolean; date?:string; available?:boolean;showEmpty?:boolean; entryId?:string|null;generation?:number|null;contextPending?:boolean;historicalSaved?:()=>Promise<void>; onDraft?:(dirty:boolean,busy:boolean)=>void; saveManual?:(part:WeatherPart|null)=>Promise<void|"verified"|"unverified">; reload?:()=>Promise<void> } = $props();
  const uid = $props.id();
  const label = (key: string) => weatherLabels[key as keyof typeof weatherLabels]?.({}, { locale }) ?? key;
  let open = $state(false);
  let form = $state(false), dirty = $state(false), busy = $state(false), failure = $state(""), reloaded = $state(false);
  let historyBusy = $state(false);
  let historyStep=$state<"overview"|"fetch"|"compare">("overview");
  let historyControl=$state<HistoricalWeather>();
  async function setHistoryStep(next:"overview"|"fetch"|"compare") {
    const hadFocus=panel?.contains(document.activeElement) ?? false;
    historyStep=next;await tick();
    if(!open||!panel)return;
    panel.scrollTop=0;
    // Keep keyboard focus inside this same panel if the old step removed it.
    if(hadFocus && document.activeElement===document.body)panel.focus({preventScroll:true});
  }
  let weatherForm = $state<WeatherForm>();
  export async function flushManual():Promise<boolean> {
    if(busy||historyBusy||unverified)return false;
    if(!dirty)return true;
    const part=weatherForm?.getPart();if(!part)return false;
    await persist(part);return !dirty && !unverified;
  }
  const unverified = $derived(status.kind === "unverified");
  function draftChanged(value:boolean) { dirty = value; onDraft(dirty,busy||historyBusy); }
  function setBusy(value:boolean) { busy = value; onDraft(dirty,busy||historyBusy); }
  function setHistoryBusy(value:boolean) { historyBusy=value;onDraft(dirty,busy||historyBusy); }
  let pending = $state<"panel"|"form"|"remove"|null>(null);
  function ask(action: "panel"|"form"|"remove") {
    pending = action;
    void tick().then(() => panel?.querySelector<HTMLButtonElement>(".confirm button")?.focus({preventScroll:true}));
  }
  function requestDiscard(target:"panel"|"form") {
    if (busy || historyBusy) return false;
    if (dirty) { ask(target); return false; }
    form = false; if (!unverified) failure = ""; return true;
  }
  function resolvePending() {
    const action = pending; pending = null;
    if (action === "remove") { void persist(null); return; }
    draftChanged(false); form = false; if (!unverified) failure = "";
    if (action === "panel") { open = false; trigger?.focus({preventScroll:true}); }
  }
  async function persist(part:WeatherPart|null) {
    if (busy || historyBusy || unverified) return;
    setBusy(true); failure = "";
    try {
      const result = await saveManual(part);
      if (result === "unverified") { failure = "unverified"; }
      else { draftChanged(false); form = false; }
    }
    catch (error) { failure = String(error) === 'weather_revision_conflict' ? 'conflict' : String(error)==='weather_new_entry_needs_text' ? 'needText' : 'saveError'; }
    finally { setBusy(false); }
  }
  async function refresh() {
    if (busy || historyBusy) return;
    setBusy(true);
    try { await reload(); failure = ""; reloaded = true; }
    catch { failure = unverified ? 'unverified' : 'saveError'; } finally { setBusy(false); }
  }

  let positioned = $state(false);
  let trigger = $state<HTMLButtonElement>();
  let panel = $state<HTMLDivElement>();
  let left = $state(16), top = $state(16);
  const sources = $derived((status.kind === "ready" || status.kind === "unverified") ? ([['manual', status.data.manual], ['fetched', status.data.fetched]] as const).filter(([, part]) => part !== null) : []);
  const empty = $derived(status.kind === "ready" && sources.length === 0);
  $effect(() => {
    const shown=open, editable=editing, current=status;
    untrack(()=>{if(shown && editable && current.kind==='ready' && !current.data.manual && !form && !dirty)form=true;});
  });
  async function toggle() {
    if (open) { close(true); return; }
    if (!trigger) return;
    positioned = false; top = 16;
    if(editing && status.kind==='ready' && !status.data.manual)form=true;
    open = true; await tick();
    if (!panel || !trigger) return;
    const position = weatherPanelPosition(trigger.getBoundingClientRect(), panel.offsetWidth, panel.offsetHeight, { width: window.innerWidth, height: window.innerHeight });
    left = position.left; top = position.top; positioned = true;
    await tick();
    panel?.querySelector<HTMLButtonElement>("button")?.focus();
  }
  async function reposition() {
    top = 16; await tick();
    if (open && panel && trigger) {
      const position = weatherPanelPosition(trigger.getBoundingClientRect(), panel.offsetWidth, panel.offsetHeight, {width:window.innerWidth,height:window.innerHeight});
      left = position.left; top = position.top;
    }
  }
  function close(returnFocus = false): boolean { if (busy || historyBusy) return false; if (form && !requestDiscard("panel")) return false; open = false; if (returnFocus) trigger?.focus({ preventScroll: true }); return true; }
  function outside(event: PointerEvent) { if (dirty && event.target instanceof Element && event.target.closest("a[href], .leave-overlay")) return; if (open && event.target instanceof Node && !panel?.contains(event.target) && !trigger?.contains(event.target) && !close()) event.preventDefault(); }
  function keydown(event: KeyboardEvent) { if (open && event.key === "Escape") { event.preventDefault(); event.stopPropagation(); if(historyStep!=="overview" && !historyBusy && !busy){void historyControl?.discardPreview();return;} if (pending) { pending = null; panel?.querySelector<HTMLButtonElement>(".close")?.focus({preventScroll:true}); } else close(true); } }
  function focusout(event: FocusEvent) { if (dirty && event.relatedTarget instanceof Element && event.relatedTarget.closest("a[href], .leave-overlay")) return; if (open && event.relatedTarget instanceof Node && !panel?.contains(event.relatedTarget) && !trigger?.contains(event.relatedTarget) && !close()) (event.target as HTMLElement)?.focus({preventScroll:true}); }
</script>
<svelte:window onpointerdown={outside} onkeydown={keydown} onresize={() => { if(open) void reposition(); }} onscroll={() => { if (!form || !dirty) close(); }} />
{#if !empty || editing || showEmpty}
<button bind:this={trigger} class="weather-tile" class:compact={empty} aria-haspopup="dialog" aria-expanded={open} aria-controls={open ? `${uid}-panel` : undefined} onclick={toggle}>
  {#if unverified}
    <span class="summary">{label('form_unverifiedShort')}</span>
  {:else if empty}
    <WeatherIcon name="neutral" size={16} />
    <span>{label('heading')}</span>
  {:else}
    {#if status.kind === "ready"}<WeatherOverview data={status.data} {locale} compact />
    {:else}<span class="heading">{label('heading')}</span><span class="summary">{label(status.kind)}</span>{/if}
  {/if}
</button>
{#if open}
  <div bind:this={panel} id={`${uid}-panel`} class="weather-panel" style:visibility={positioned ? "visible" : "hidden"} role="dialog" aria-modal="false" aria-labelledby={`${uid}-title`} style:left={`${left}px`} style:top={`${top}px`} style:max-height={`calc(100vh - ${top + 16}px)`} onfocusout={focusout} tabindex="-1">
    <header><h2 id={`${uid}-title`}>{label('heading')}</h2><button class="close" aria-label={label('close')} onclick={() => close(true)}>×</button></header>
    {#if status.kind !== "ready" && status.kind !== "unverified"}<p role="status">{label(status.kind)}</p>
    {:else}
      {#if !editing && !unverified && sources.length === 0}<p>{label('empty')}</p>{/if}
      <div class="sources" class:paired={historyStep === 'overview' && (editing || (!unverified && sources.length === 2))}>
        {#if editing}
          <section hidden={historyStep !== 'overview'} aria-label={label('observations')}><div class="source-heading"><h3>{label('observations')}</h3>
            {#if available && !form && !unverified}<button class="edit-control" type="button" disabled={busy} onclick={() => {form=true; failure=""; reloaded=false;}}>{label(status.data.manual ? 'form_edit' : 'form_add')}</button>{/if}</div>
            {#if !available}<p>{label('form_needEntry')}</p>
            {:else if form}<WeatherForm bind:this={weatherForm} manual={status.data.manual} {date} {locale} busy={busy || historyBusy || unverified} onSave={persist} onCancel={() => requestDiscard("form")} onDirty={draftChanged} />
            {:else}
              {#if !unverified && status.data.manual}<WeatherDetails part={status.data.manual} {locale} manual />{/if}
              {#if !unverified && status.data.manual}<button class="remove-control" type="button" disabled={busy} onclick={() => ask('remove')}>{label('form_remove')}</button>{/if}
            {/if}
            {#if reloaded && form && !unverified}<details><summary>{label('form_currentValues')}</summary>{#if status.data.manual}<WeatherDetails part={status.data.manual} {locale} manual />{:else}<p>{label('empty')}</p>{/if}</details>{/if}
            {#if busy}<p role="status">{label('form_saving')}</p>{/if}
            {#if failure && !unverified}<p role="alert">{label(`form_${failure}`)}</p>{#if failure === 'conflict'}<button type="button" disabled={busy} onclick={refresh}>{label('form_reload')}</button>{/if}{/if}
          </section>
        {:else if !unverified && status.data.manual}<section aria-label={label('observations')}><h3>{label('observations')}</h3><WeatherDetails part={status.data.manual} {locale} manual /></section>{/if}
        {#if editing || (!unverified && status.data.fetched)}
          <section class="historical-column" aria-label={label('history_title')}>
            {#if editing && generation !== null}
              <HistoricalWeather bind:this={historyControl} {entryId} {generation} {date} {locale} revision={status.data.revision} existing={unverified ? null : status.data.fetched} {contextPending} blocked={busy||unverified||contextPending} onBusy={setHistoryBusy} onSaved={historicalSaved} onConflict={historicalSaved} onStep={setHistoryStep} onDone={()=>close(true)}/>
            {:else if !unverified && status.data.fetched}<div class="stored-weather"><h3>{label('meteo')}</h3><WeatherDetails part={status.data.fetched} {locale} /></div>{/if}
          </section>
        {/if}
      </div>
      {#if unverified}<p role="alert">{label('form_unverified')}</p><button type="button" disabled={busy} onclick={refresh}>{label('form_reload')}</button>{/if}
    {/if}
    {#if pending}
      <div class="confirm" role="alertdialog" aria-modal="false" aria-labelledby={`${uid}-confirm`}>
        <p id={`${uid}-confirm`}>{label(pending === 'remove' ? 'form_removeConfirm' : 'form_discardConfirm')}</p>
        <button type="button" onclick={resolvePending}>{label(pending === 'remove' ? 'form_confirmRemove' : 'form_discard')}</button>
        <button type="button" onclick={() => {pending = null; panel?.querySelector<HTMLButtonElement>(".close")?.focus({preventScroll:true});}}>{label('form_keep')}</button>
      </div>
    {/if}
  </div>
{/if}
{/if}
<style>
  .weather-tile, .weather-tile.compact { display:inline-flex;align-items:center;gap:6px;width:auto;height:auto;margin:0;padding:4px 6px;border:1px solid transparent;border-radius:6px;background:transparent;color:var(--accent,#435b47);font:12px/1.4 system-ui,sans-serif;cursor:pointer; }
  .weather-tile:hover { border-color: #788e7b; }
  button:focus-visible { outline: 2px solid #435b47; outline-offset: 3px; }
  .heading { display: block; font-size: 10px; letter-spacing: 1px; text-transform: uppercase; margin-bottom: 7px; }
  .summary { display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 2; line-clamp: 2; overflow: hidden; }
  .weather-panel { position: fixed; z-index: 1000; box-sizing: border-box; width: min(1000px, calc(100vw - 32px)); max-height: calc(100vh - 32px); overflow: auto; overscroll-behavior: contain; padding: 20px 24px; border: 1px solid #d9d8d1; border-radius: 14px; background: #faf9f5; box-shadow: 0 12px 40px #34353026; color: #343530; font: 13px/1.5 system-ui, sans-serif; text-align: left; }
  .confirm { position: fixed; z-index: 1100; top: 50%; left: 50%; transform: translate(-50%, -50%); box-sizing: border-box; width: min(360px, calc(100vw - 32px)); padding: 20px; border: 1px solid #788e7b; background: #faf9f5; border-radius: 8px; box-shadow: 0 4px 24px #34353055; } .confirm button { margin: 4px 8px 0 0; padding: 7px 12px; }
  header { position:sticky;top:0;z-index:2;background:#faf9f5;padding:8px 0;display: flex; align-items: center; justify-content: space-between; gap: 16px; }
  .weather-panel h2 { margin: 0; font: 500 16px/1.4 system-ui, sans-serif; }
  .sources { margin-top: 20px; }
  .sources.paired { display: grid; grid-template-columns: minmax(0,1fr) minmax(0,1fr); gap: 24px; }
  .sources section { min-width: 0; }
  .weather-panel :global(.description.manual) { max-height:none;overflow:visible; }
  .stored-weather { margin-top:24px;padding-top:20px;border-top:1px solid #e2e2d9; }
  .sources.paired > section + section { border-left: 1px solid #e2e2d9; padding-left: 24px; }
  .weather-panel h3 { margin: 0 0 16px; font-size: 13px; font-weight: 600; color: #435b47; }
  .source-heading { display:flex; align-items:baseline; justify-content:space-between; gap:8px; }
  .edit-control { border:0; background:transparent; color:#435b47; font:inherit; padding:4px 6px; cursor:pointer; } .edit-control:hover:not(:disabled) { background:#eef1eb; } .edit-control:disabled { opacity:.55; cursor:default; }
  .remove-control { margin-top:12px; border:1px solid #d9d8d1; border-radius:6px; background:transparent; color:var(--text, #252b27); font:inherit; padding:7px 12px; cursor:pointer; } .remove-control:hover:not(:disabled) { background:#eef1eb; } .remove-control:disabled { opacity:.55; cursor:default; }
  .close { border: 0; background: transparent; color: #435b47; font-size: 24px; cursor: pointer; min-width: 32px; min-height: 32px; }
  @media (max-width: 800px) { .sources.paired { grid-template-columns: minmax(0,1fr); gap: 24px; } .sources.paired > section + section { border-left: 0; border-top: 1px solid #e2e2d9; padding: 24px 0 0; } .weather-panel { padding: 16px 20px; } }

</style>
