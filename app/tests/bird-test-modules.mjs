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
export const pasteUrl=url(resolve(transpile(source('../src/lib/bird-paste.ts')),{'$lib/birds':birdsUrl}));
export const paste=await import(pasteUrl);
export const photoUrl=url(resolve(transpile(source('../src/lib/bird-photo.ts')),{}));
export const photo=await import(photoUrl);
const messages=new URL('../src/lib/paraglide/messages.js',import.meta.url).href;
function componentModule(text,aliases){const compiled=compile(text,{generate:'server'});if(compiled.warnings.length)throw Error(JSON.stringify(compiled.warnings));return url(resolve(compiled.js.code,aliases));}
export function pasteReviewUrl(text=source('../src/lib/components/BirdPasteReview.svelte')){return componentModule(text,{'$lib/birds':birdsUrl,'$lib/bird-names':namesUrl,'$lib/bird-paste':pasteUrl,'$lib/paraglide/messages.js':messages});}
export const galleryUrl=url(resolve(transpile(source('../src/lib/photo-gallery.ts')),{'$lib/birds':birdsUrl,'$lib/bird-names':namesUrl}));
export const gallery=await import(galleryUrl);
export const aboutUrl=componentModule(source('../src/lib/components/PhotoAbout.svelte'),{'$lib/paraglide/messages.js':messages});
export const viewerUrl=componentModule(source('../src/lib/components/PhotoViewer.svelte'),{'$lib/photo-gallery':galleryUrl,'$lib/components/PhotoAbout.svelte':aboutUrl,'$lib/paraglide/messages.js':messages});
export const scopeUrl=componentModule(source('../src/lib/components/PhotoScope.svelte'),{'$lib/photo-gallery':galleryUrl,'$lib/components/PhotoViewer.svelte':viewerUrl});
export const thumbnailUrl=componentModule(source('../src/lib/components/PhotoThumbnail.svelte'),{'$lib/photo-gallery':galleryUrl,'$lib/components/PhotoViewer.svelte':viewerUrl,'$lib/paraglide/messages.js':messages});
export const journalGalleryUrl=url('export async function loadJournalGallery(){throw Error("SSR must not load galleries");}');
export const journalScopeUrl=componentModule(source('../src/lib/components/JournalPhotoScope.svelte'),{'$lib/components/PhotoScope.svelte':scopeUrl,'$lib/journal-photo-gallery':journalGalleryUrl});
export const photoViewUrl=componentModule(source('../src/lib/components/BirdPhotoView.svelte'),{'$lib/bird-photo':photoUrl,'$lib/components/PhotoThumbnail.svelte':thumbnailUrl,'$lib/components/PhotoAbout.svelte':aboutUrl,'$lib/paraglide/messages.js':messages});
export const rowPhotoUrl=componentModule(source('../src/lib/components/BirdRowPhoto.svelte'),{'$lib/birds':birdsUrl,'$lib/bird-photo':photoUrl,'$lib/components/BirdPhotoView.svelte':photoViewUrl,'$lib/components/PhotoAbout.svelte':aboutUrl,'$lib/paraglide/messages.js':messages});
export const visitPreviewUrl=componentModule(source('../src/lib/components/BirdVisitPreview.svelte'),{'$lib/birds':birdsUrl,'$lib/bird-names':namesUrl,'$lib/paraglide/messages.js':messages});
export const linkPickerUrl=componentModule(source('../src/lib/components/BirdLinkPicker.svelte'),{'$lib/birds':birdsUrl,'$lib/components/BirdVisitPreview.svelte':visitPreviewUrl,'$lib/paraglide/messages.js':messages});
export function previewUrl(seeded){const compiled=compile(seeded??source('../src/lib/components/BirdJournalLists.svelte'),{generate:'server',filename:'BirdJournalLists.svelte'});if(compiled.warnings.length)throw Error(JSON.stringify(compiled.warnings));return url(resolve(compiled.js.code,{'$lib/photo-gallery':galleryUrl,'$lib/components/PhotoThumbnail.svelte':thumbnailUrl,'$lib/birds':birdsUrl,'$lib/bird-names':namesUrl,'$lib/components/BirdVisitPreview.svelte':visitPreviewUrl,'$lib/components/BirdLinkPicker.svelte':linkPickerUrl,'$lib/components/DeleteEntryDialog.svelte':componentModule(source('../src/lib/components/DeleteEntryDialog.svelte'),{'$lib/paraglide/messages.js':messages}),'$lib/paraglide/messages.js':messages}));}
export const birdPreviewUrl=previewUrl();

// Use the real DatePicker: controller-only tests miss premature child rendering.
export function creationUrl(text, locale='sv',entryId='origin') {
 const language=url(`export const language={current:${JSON.stringify(locale)}};`);
 const messages=new URL('../src/lib/paraglide/messages.js',import.meta.url).href;
 const component=(text,aliases)=>url(resolve(compile(text,{generate:'server'}).js.code,aliases));
 const picker=component(source('../src/lib/components/DatePicker.svelte'),{'$lib/language.svelte':language,'$lib/paraglide/messages.js':messages});
 const attribution=component(source('../src/lib/components/BirdAttribution.svelte'),{'$lib/paraglide/messages.js':messages});
 return component(text,{
  '$lib/language.svelte':language,'$lib/paraglide/messages.js':messages,'$lib/birds':birdsUrl,'$lib/bird-names':namesUrl,'$lib/components/EntryOptions.svelte':url(resolve(compile(source('../src/lib/components/EntryOptions.svelte'),{generate:'server'}).js.code,{'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href})),'$lib/components/BirdSpeciesName.svelte':speciesNameUrl,
  '$lib/journal-data':reader,'$lib/places':url('export async function readJournalPlace(){}'),
  '$lib/place-map.svelte':url('export const PLACE_MAP="map";'),
  '$app/state':url(`export const page={params:{id:${JSON.stringify(entryId)}}};`),
  '$app/navigation':url('export function beforeNavigate(){};export async function goto(){}'),
  '$lib/components/BirdRowPhoto.svelte':rowPhotoUrl,'$lib/components/BirdPasteReview.svelte':pasteReviewUrl(),'$lib/components/VisitTimePicker.svelte':timePickerUrl,'$lib/components/DatePicker.svelte':picker,'$lib/components/BirdAttribution.svelte':attribution
 });
}

export const speciesNameUrl=url(resolve(compile(source('../src/lib/components/BirdSpeciesName.svelte'),{generate:'server'}).js.code,{'$lib/bird-names':namesUrl,'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href}));
export function detailUrl(){return url(resolve(compile(source('../src/lib/components/BirdListDetail.svelte'),{generate:'server'}).js.code,{'$lib/photo-gallery':galleryUrl,'$lib/components/PhotoScope.svelte':scopeUrl,'$lib/bird-names':namesUrl,'$lib/components/EntryOptions.svelte':url(resolve(compile(source('../src/lib/components/EntryOptions.svelte'),{generate:'server'}).js.code,{'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href})),'$lib/components/BirdSpeciesName.svelte':speciesNameUrl,'$lib/components/BirdPhotoView.svelte':photoViewUrl,'$lib/components/BirdAttribution.svelte':url(resolve(compile(source('../src/lib/components/BirdAttribution.svelte'),{generate:'server'}).js.code,{'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href})),'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href}));}

export const deleteDialogUrl=url(resolve(compile(source('../src/lib/components/DeleteEntryDialog.svelte'),{generate:'server'}).js.code,{'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href}));

export const timePickerUrl=url(resolve(compile(source('../src/lib/components/VisitTimePicker.svelte'),{generate:'server'}).js.code,{'$lib/birds':birdsUrl,'$lib/paraglide/messages.js':new URL('../src/lib/paraglide/messages.js',import.meta.url).href}));

export function overviewUrl(text,locale='sv'){
 return componentModule(text,{'$lib/bird-names':namesUrl,'$lib/language.svelte':url(`export const language={current:${JSON.stringify(locale)}};`),'$lib/birds':birdsUrl,'$lib/components/BirdVisitPreview.svelte':visitPreviewUrl,'$lib/paraglide/messages.js':messages});
}

export function journalLinksUrl(text){return componentModule(text,{'$lib/birds':birdsUrl,'$lib/bird-names':namesUrl,'$lib/components/DeleteEntryDialog.svelte':deleteDialogUrl,'$lib/paraglide/messages.js':messages});}
