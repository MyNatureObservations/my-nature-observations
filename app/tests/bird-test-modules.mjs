import { readFileSync } from 'node:fs';
import ts from 'typescript';
import { compile } from 'svelte/compiler';
export const source=p=>readFileSync(new URL(p,import.meta.url),'utf8');
export const url=s=>`data:text/javascript;base64,${Buffer.from(s).toString('base64')}`;
export const transpile=s=>ts.transpileModule(s,{compilerOptions:{target:ts.ScriptTarget.ESNext,module:ts.ModuleKind.ESNext}}).outputText;
const resolve=(s,local)=>s.replace(/from (["'])([^"']+)\1/g,(_,q,n)=>`from ${q}${local[n]??import.meta.resolve(n)}${q}`);
const reader=url('export function createJournalReader(){return async()=>{throw Error("SSR read");};}');
export const birdsUrl=url(resolve(transpile(source('../src/lib/birds.ts')),{'$lib/journal-data':reader}));
export const birds=await import(birdsUrl);
export const namesUrl=url(resolve(transpile(source('../src/lib/bird-names.ts')),{}));
export const names=await import(namesUrl);
export function previewUrl(seeded){const compiled=compile(seeded??source('../src/lib/components/BirdJournalLists.svelte'),{generate:'server',filename:'BirdJournalLists.svelte'});if(compiled.warnings.length)throw Error(JSON.stringify(compiled.warnings));return url(resolve(compiled.js.code,{'$lib/birds':birdsUrl,'$lib/bird-names':namesUrl,'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href}));}
export const birdPreviewUrl=previewUrl();

// Use the real DatePicker: controller-only tests miss premature child rendering.
export function creationUrl(text, locale='sv') {
 const language=url(`export const language={current:${JSON.stringify(locale)}};`);
 const messages=new URL('../src/lib/paraglide/messages.js',import.meta.url).href;
 const component=(text,aliases)=>url(resolve(compile(text,{generate:'server'}).js.code,aliases));
 const picker=component(source('../src/lib/components/DatePicker.svelte'),{'$lib/language.svelte':language,'$lib/paraglide/messages.js':messages});
 const attribution=component(source('../src/lib/components/BirdAttribution.svelte'),{'$lib/paraglide/messages.js':messages});
 return component(text,{
  '$lib/language.svelte':language,'$lib/paraglide/messages.js':messages,'$lib/birds':birdsUrl,'$lib/bird-names':namesUrl,'$lib/components/EntryOptions.svelte':url(resolve(compile(source('../src/lib/components/EntryOptions.svelte'),{generate:'server'}).js.code,{'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href})),'$lib/components/BirdSpeciesName.svelte':speciesNameUrl,
  '$lib/journal-data':reader,'$lib/places':url('export async function readJournalPlace(){}'),
  '$lib/place-map.svelte':url('export const PLACE_MAP="map";'),
  '$app/state':url('export const page={params:{id:"origin"}};'),
  '$app/navigation':url('export function beforeNavigate(){};export async function goto(){}'),
  '$lib/components/VisitTimePicker.svelte':timePickerUrl,'$lib/components/DatePicker.svelte':picker,'$lib/components/BirdAttribution.svelte':attribution
 });
}

export const speciesNameUrl=url(resolve(compile(source('../src/lib/components/BirdSpeciesName.svelte'),{generate:'server'}).js.code,{'$lib/bird-names':namesUrl,'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href}));
export function detailUrl(){return url(resolve(compile(source('../src/lib/components/BirdListDetail.svelte'),{generate:'server'}).js.code,{'$lib/bird-names':namesUrl,'$lib/components/EntryOptions.svelte':url(resolve(compile(source('../src/lib/components/EntryOptions.svelte'),{generate:'server'}).js.code,{'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href})),'$lib/components/BirdSpeciesName.svelte':speciesNameUrl,'$lib/components/BirdAttribution.svelte':url(resolve(compile(source('../src/lib/components/BirdAttribution.svelte'),{generate:'server'}).js.code,{'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href})),'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href}));}

export const deleteDialogUrl=url(resolve(compile(source('../src/lib/components/DeleteEntryDialog.svelte'),{generate:'server'}).js.code,{'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href}));

export const timePickerUrl=url(resolve(compile(source('../src/lib/components/VisitTimePicker.svelte'),{generate:'server'}).js.code,{'$lib/birds':birdsUrl,'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href}));
