import type {BirdList} from "$lib/birds";
import {groupBirdVisits,speciesLabel} from "$lib/birds";
import {formatBirdDate,capitalizeCommonName} from "$lib/bird-names";
export type GalleryPhoto={key:string;src:string|null;alt:string;caption:string;description:string;width?:number;height?:number};
export const PHOTO_OPEN_EVENT="mno:photo-open";
export function observationPhotos(lists:BirdList[],locale:"sv"|"en"):GalleryPhoto[]{
 const seen=new Set<string>();const output:GalleryPhoto[]=[];
 for(const list of groupBirdVisits(lists).flatMap(group=>group.visits)){
  if(seen.has(list.id))continue;seen.add(list.id);
  list.rows.forEach((row,i)=>{if(!row.photo)return;const name=capitalizeCommonName(speciesLabel(row,locale),locale);output.push({key:`bird:${list.id}:${row.rowId??i}:${row.photo.asset_id}`,src:`data:${row.photo.mime};base64,${row.photo.preview}`,alt:name,caption:row.photo.caption??"",description:`${name} · ${list.place.name} · ${formatBirdDate(list.visitDate,locale)}`,width:row.photo.width,height:row.photo.height});});
 }
 return output;
}
export type JournalPhotoRef={nodeId:string;assetId:string;caption?:string;alt?:string};
export function journalPhotoRefs(contentJson:string):JournalPhotoRef[]{
 let doc;try{doc=JSON.parse(contentJson);}catch{return [];}
 const refs:JournalPhotoRef[]=[];const visit=(node:unknown)=>{if(!node||typeof node!=="object")return;const n=node as {type?:string;attrs?:JournalPhotoRef;content?:unknown[]};if(n.type==="journalImage"&&typeof n.attrs?.nodeId==="string"&&typeof n.attrs?.assetId==="string")refs.push(n.attrs);if(Array.isArray(n.content))n.content.forEach(visit);};visit(doc);return refs;
}
export function photoGalleryRequest(){let revision=0;return {start(){const own=++revision;return ()=>own===revision;},invalidate(){revision++;}};}
