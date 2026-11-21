import {sourceTime,type Visit,type SourceRow} from '$lib/ebird-preview';
import {formatBirdDate,formatBirdCount} from '$lib/bird-names';
export type VisitDay={date:string;visits:Visit[]};
function dayValue(value:string):number|null{
 if(!/^\d{4}-\d{2}-\d{2}$/.test(value))return null;
 const time=Date.parse(value+'T00:00:00Z');
 return Number.isFinite(time)&&new Date(time).toISOString().slice(0,10)===value?time:null;
}
export function startMinutes(value:string):number|null{
 const time=sourceTime(value);return time?Number(time.slice(0,2))*60+Number(time.slice(3)):null;
}
const compare=(a:string,b:string)=>a<b?-1:a>b?1:0;
export function groupPreviewVisits(visits:Visit[]):VisitDay[]{
 const groups=new Map<string,Visit[]>();
 for(const visit of visits){const date=visit.fields.Date??'';const group=groups.get(date)??[];group.push(visit);groups.set(date,group);}
 const days=[...groups].map(([date,visits])=>({date,visits:[...visits].sort((a,b)=>{
  const left=startMinutes(a.fields.Time??''),right=startMinutes(b.fields.Time??'');
  return Number(left===null)-Number(right===null)||(left??0)-(right??0)||compare(a.id,b.id);
 })}));
 return days.sort((a,b)=>{const left=dayValue(a.date),right=dayValue(b.date);return Number(left===null)-Number(right===null)||(right??0)-(left??0)||compare(a.date,b.date);});
}
export function formatSourceDate(value:string,locale:'sv'|'en'):string{return formatBirdDate(value,locale);}
export function formatSourceTime(value:string,locale:'sv'|'en'):string{
 const minutes=startMinutes(value);if(minutes===null)return value;
 const date=new Date('2000-01-01T00:00:00Z');date.setUTCMinutes(minutes);
 return new Intl.DateTimeFormat(locale==='sv'?'sv-SE':'en-GB',{hour:'2-digit',minute:'2-digit',hourCycle:'h23',timeZone:'UTC'}).format(date);
}
export function sourceSpeciesName(row:SourceRow):string{return row.fields['Common Name']||row.fields['Scientific Name'];}
export function compactSpecies(visit:Visit):{rows:SourceRow[];more:number}{return {rows:visit.rows.slice(0,3),more:Math.max(0,visit.rows.length-3)};}
export function displaySourceCount(value:string,locale:'sv'|'en'):string{return /^[1-9]\d*$/.test(value)?formatBirdCount(value,locale):value;}
