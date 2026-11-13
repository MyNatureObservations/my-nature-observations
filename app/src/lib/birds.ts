import { invoke } from "@tauri-apps/api/core";
import { createJournalReader } from "$lib/journal-data";
import type { Place } from "$lib/places";
export const BIRD_CATALOG_VERSION = "AviList v2026";
export type Species = { id: string; english: string; swedish: string | null; scientific: string; referral?: string };
export type BirdRow = Species & { count: string; taxonomyVersion?:string; comment?:string; commentOpen?:boolean };
export type ListRow = Omit<Species, "id"> & { speciesId: string; taxonomyVersion: string; count: string; comment:string|null };
export type BirdList = { id: string; editToken:string; visitDate: string; visitStart: string | null; visitEnd: string | null; place: Place; rows: ListRow[] };
export function speciesLabel(s: Pick<Species,"english"|"swedish"|"scientific">, locale: "sv"|"en"): string {
  return locale === "sv" ? s.swedish ?? `${s.english} · ${s.scientific}` : s.english;
}
export function validDate(s: string): boolean { if (!/^\d{4}-\d{2}-\d{2}$/.test(s)||s.startsWith("0000")) return false; const d=new Date(`${s}T12:00:00Z`);return Number.isFinite(d.getTime())&&d.toISOString().slice(0,10)===s; }
export function validTime(s:string):boolean{return !s||/^(?:[01]\d|2[0-3]):[0-5]\d$/.test(s);}
export function validBirdCount(count:string):boolean{return /^[1-9]\d{0,18}$/.test(count)&&BigInt(count)<=9223372036854775807n;}
export function validBirdComment(value:string|undefined):boolean{return value===undefined||(!value.includes("\0")&&new TextEncoder().encode(value).length<=8192);}
export function canSaveBirdList(date:string,place:Place|null,rows:BirdRow[],start:string,end:string):boolean {
 return validDate(date)&&!!place&&place.latitude!==null&&place.longitude!==null&&Number.isFinite(place.latitude)&&Number.isFinite(place.longitude)&&Math.abs(place.latitude)<=90&&Math.abs(place.longitude)<=180&&validTime(start)&&validTime(end)&&rows.length>0&&rows.every(r=>validBirdCount(r.count)&&validBirdComment(r.comment));
}
export function entryHref(id:string):string{return `/journal/entries/${encodeURIComponent(id)}`;}
export function birdHref(entry:string,id:string):string{return `${entryHref(entry)}/birds/${encodeURIComponent(id)}`;}
export async function saveBirdList(generation:number,entryId:string,date:string,place:Place,rows:BirdRow[],start:string,end:string,requestId:string):Promise<string>{
 const id=await invoke<string>("create_journal_bird_list",{generation,requestId,entryId,visitDate:date,placeId:place.id,rows:rows.map(r=>({speciesId:r.id,taxonomyVersion:BIRD_CATALOG_VERSION,count:r.count,comment:r.comment||null})),visitStart:start||null,visitEnd:end||null});
 if(id!==requestId)throw Error("bird_save_response_invalid");
 return id;
}
export const makeBirdReader=()=>createJournalReader();

export async function confirmSavedBirdList(generation:number,entryId:string,id:string):Promise<BirdList>{
 const response=await createJournalReader()<BirdList>("read_journal_bird_list",{generation,entryId,id});
 if(response.generation!==generation||response.data.id!==id)throw Error("bird_save_confirmation_invalid");
 return response.data;
}

export async function updateBirdList(generation:number,entryId:string,id:string,expectedToken:string,date:string,place:Place,rows:BirdRow[],start:string,end:string):Promise<string>{
 const token=await invoke<string>("update_journal_bird_list",{generation,entryId,id,expectedToken,visitDate:date,placeId:place.id,rows:rows.map(r=>({speciesId:r.id,taxonomyVersion:r.taxonomyVersion??BIRD_CATALOG_VERSION,count:r.count,comment:r.comment||null})),visitStart:start||null,visitEnd:end||null});
 if(typeof token!=="string"||!/^[a-f0-9]{64}$/.test(token))throw Error("bird_edit_response_invalid");
 return token;
}

export async function deleteBirdList(generation:number,id:string,expectedToken:string):Promise<void>{
 await invoke<void>("delete_journal_bird_list",{generation,id,expectedToken,confirmed:true});
}
export async function birdListExists(generation:number,id:string):Promise<boolean>{
 const result=await createJournalReader()<boolean>("journal_bird_list_exists",{generation,id});
 if(result.generation!==generation||typeof result.data!=="boolean")throw Error("bird_delete_confirmation_invalid");
 return result.data;
}

export async function reserveBirdRequest(generation:number):Promise<string>{
 const id=await invoke<string>("reserve_bird_creation",{generation});
 if(typeof id!=="string"||!/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/.test(id))throw Error("bird_request_invalid");
 return id;
}
