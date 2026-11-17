import {invoke} from "@tauri-apps/api/core";
import {createJournalReader} from "$lib/journal-data";
import {readPhotograph,photographSource} from "$lib/journal-photographs";
import type {BirdList} from "$lib/birds";
import {journalPhotoRefs,observationPhotos,type GalleryPhoto} from "$lib/photo-gallery";
export async function loadJournalGallery(contentJson:string,entryId:string,generation:number|null,locale:"sv"|"en"):Promise<{items:GalleryPhoto[];incomplete:boolean}>{
 if(generation===null)throw Error("journal_generation_conflict");
 const refs=journalPhotoRefs(contentJson);
 const journal=await Promise.all(refs.map(async attrs=>{const item:GalleryPhoto={key:`journal:${attrs.nodeId}`,src:null,alt:attrs.alt??"",caption:attrs.caption??"",description:""};try{const asset=await readPhotograph(invoke,entryId,generation,attrs.assetId);return {...item,src:photographSource(asset),width:asset.width,height:asset.height};}catch{return item;}}));
 let linked:BirdList[]=[],incomplete=false;
 try{if(await invoke<boolean>("bird_observations_available")){const response=await createJournalReader()<BirdList[]>("read_journal_bird_lists",{entryId,generation});if(response.generation!==generation)throw Error("journal_generation_conflict");linked=response.data;}}catch{incomplete=true;}
 const current=await invoke<{generation:number}>("new_journal_session");if(current.generation!==generation)throw Error("journal_generation_conflict");
 return {items:[...journal,...observationPhotos(linked,locale)],incomplete};
}
