import {BIRD_CATALOG_VERSION,validBirdCount,type Species,type BirdRow} from "$lib/birds";
// Presentation only: the existing strict integer validator remains authoritative.
export function pasteCountError(count:string):null|"required"|"minimum"{
 if(validBirdCount(count))return null;
 return /^[+-]?(?:\d+(?:[.,]\d*)?|[.,]\d+)(?:[eE][+-]?\d+)?$/.test(count.trim())?"minimum":"required";
}
export const PASTE_MAX_LINES=200;
export const PASTE_MAX_BYTES=64*1024;
export type ParsedBirdLine={line:number;original:string;name:string;count:string};
export type PasteRow=ParsedBirdLine&{candidates:Species[];selected:Species|null;excluded:boolean;combineInto:string};
export function parseBirdPaste(text:string):{rows:ParsedBirdLine[];blanks:number;limited:boolean}{
 if(new TextEncoder().encode(text).length>PASTE_MAX_BYTES)return {rows:[],blanks:0,limited:true};
 const rows:ParsedBirdLine[]=[];let blanks=0;
 for(const [i,original] of text.split(/\r\n|\r|\n/).entries()){
  const value=original.trim();if(!value){blanks++;continue;}
  const match=/^(.*?)\s+(\S+)(?:\s+st)?$/i.exec(value);
  const name=match?match[1]:value;
  const count=match?match[2].replace(/(?<=\d)st$/i,""):"";
  rows.push({line:i+1,original,name,count});
 }
 return {rows,blanks,limited:rows.length>PASTE_MAX_LINES};
}
export function pasteLookupNames(rows:ParsedBirdLine[]):string[]{return rows.map(row=>new TextEncoder().encode(row.name).length<=200?row.name:"");}
export function prepareBirdPaste(rows:ParsedBirdLine[],matches:Species[][]):PasteRow[]{
 if(matches.length!==rows.length)throw Error("bird_paste_match_response_invalid");
 return rows.map((row,i)=>({...row,candidates:matches[i],selected:matches[i].length===1&&!matches[i][0].referral?matches[i][0]:null,excluded:false,combineInto:""}));
}
export type PasteTarget={key:string;count:string;species:Species;existingRow:number|null;sourceLine:number|null;taxonomyVersion:string};
export type PasteCheck={issue:null|"species"|"count"|"duplicate"|"target"|"overflow";targets:PasteTarget[];resultCount?:string};
export function planBirdPaste(existing:BirdRow[],review:PasteRow[]):{rows:BirdRow[];checks:PasteCheck[];canApply:boolean}{
 // Work only on copies. Legacy duplicates, order, identities and comments stay intact.
 const output:{key:string;row:BirdRow;existingRow:number|null;sourceLine:number|null}[]=existing.map((row,i)=>({key:`existing:${i}`,row:{...row},existingRow:i+1,sourceLine:null as number|null}));
 const checks:PasteCheck[]=[];let included=0;
 for(const incoming of review){
  if(incoming.excluded){checks.push({issue:null,targets:[]});continue;}
  included++;
  if(!incoming.selected){checks.push({issue:"species",targets:[]});continue;}
  const matches=output.filter(item=>item.row.id===incoming.selected!.id);
  const targets=matches.map(item=>({key:item.key,count:item.row.count,species:item.row,existingRow:item.existingRow,sourceLine:item.sourceLine,taxonomyVersion:item.row.taxonomyVersion??BIRD_CATALOG_VERSION}));
  if(!validBirdCount(incoming.count)){checks.push({issue:"count",targets});continue;}
  if(matches.length||incoming.combineInto){
   const target=matches.find(item=>item.key===incoming.combineInto);
   if(!target){checks.push({issue:"duplicate",targets});continue;}
   if(!validBirdCount(target.row.count)){checks.push({issue:"target",targets});continue;}
   const sum=BigInt(target.row.count)+BigInt(incoming.count);
   if(sum>9223372036854775807n){checks.push({issue:"overflow",targets});continue;}
   target.row={...target.row,count:String(sum)};
   checks.push({issue:null,targets,resultCount:String(sum)});
  }else{
   output.push({key:`paste:${incoming.line}`,row:{...incoming.selected,taxonomyVersion:BIRD_CATALOG_VERSION,count:incoming.count,comment:"",commentOpen:false},existingRow:null,sourceLine:incoming.line});
   checks.push({issue:null,targets});
  }
 }
 return {rows:output.map(item=>item.row),checks,canApply:included>0&&checks.every(check=>check.issue===null)};
}
export function applyBirdPaste(existing:BirdRow[],review:PasteRow[]):BirdRow[]{const plan=planBirdPaste(existing,review);if(!plan.canApply)throw Error("bird_paste_review_incomplete");return plan.rows;}
