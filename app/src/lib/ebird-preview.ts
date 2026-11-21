import type {Species} from '$lib/birds';
export type Issue={line:number;field:string;code:'csv'|'header'|'required'|'count'|'date'|'time'|'coordinate'|'number'|'metadata'|'media'|'duplicate'|'limit'};
export type SourceRow={line:number;fields:Record<string,string>;count:string|null;media:string[];catalogMatch?:Species};
export type Visit={id:string;fields:Record<string,string>;rows:SourceRow[]};
export type Preview={visits:Visit[];rows:SourceRow[];issues:Issue[];unsupported:boolean;places:number;unspecified:number;unmatched:string[];media:number};
const required=['Submission ID','Common Name','Scientific Name','Count','Location ID','Location','Latitude','Longitude','Date','Time'];
const metadata=['State/Province','County','Location ID','Location','Latitude','Longitude','Date','Time','Protocol','Duration (Min)','All Obs Reported','Distance Traveled (km)','Area Covered (ha)','Number of Observers','Checklist Comments'];
const optional=new Set(['State/Province','County','Time','Protocol','Duration (Min)','All Obs Reported','Distance Traveled (km)','Area Covered (ha)','Number of Observers','Breeding Code','Observation Details','Checklist Comments','ML Catalog Numbers','Taxonomic Order']);
// Strict CSV grammar: escaped quotes/newlines belong to fields; physical line
// numbers are retained for issues. Never split on commas or invent missing columns.
export function csvRecords(text:string):{values:string[];line:number}[]{
 text=text.replace(/^\uFEFF/,'');const out:{values:string[];line:number}[]=[];
 let row:string[]=[],field='',quoted=false,closed=false,line=1,start=1,started=false;
 const pushField=()=>{row.push(field);field='';closed=false;};
 for(let i=0;i<text.length;i++){
  const c=text[i];
  if(quoted){if(c==='"'){if(text[i+1]==='"'){field+='"';i++;}else{quoted=false;closed=true;}}else{field+=c;if(c==='\n'||(c==='\r'&&text[i+1]!=='\n'))line++;}continue;}
  if(c==='"'){if(field||closed)throw {line,code:'csv'};quoted=true;started=true;}
  else if(c===','){pushField();started=true;}
  else if(c==='\r'||c==='\n'){
   pushField();out.push({values:row,line:start});row=[];started=false;
   if(c==='\r'&&text[i+1]==='\n')i++;line++;start=line;
  }else{if(closed)throw {line,code:'csv'};field+=c;started=true;}
  if(out.length>100001)throw {line,code:'limit'};
 }
 if(quoted)throw {line:start,code:'csv'};
 if(started||row.length||closed){pushField();out.push({values:row,line:start});}
 if(out.length>100001)throw {line,code:"limit"};
 return out;
}
export function sourceTime(value:string):string|null{
 if(!value)return '';
 const m=/^(\d{1,2}):(\d{2}) ([AP]M)$/.exec(value);
 if(!m||+m[1]<1||+m[1]>12||+m[2]>59)return null;
 return `${String((+m[1]%12)+(m[3]==='PM'?12:0)).padStart(2,'0')}:${m[2]}`;
}
function dateValid(s:string):boolean{if(!/^\d{4}-\d{2}-\d{2}$/.test(s)||s.startsWith('0000'))return false;const d=new Date(s+'T12:00:00Z');return Number.isFinite(d.getTime())&&d.toISOString().slice(0,10)===s;}
function number(s:string):boolean{return /^(?:\d+(?:\.\d*)?|\.\d+)$/.test(s)&&Number.isFinite(Number(s));}
function comparable(field:string,s:string):string{
 if(['Latitude','Longitude','Duration (Min)','Distance Traveled (km)','Area Covered (ha)','Number of Observers'].includes(field)&&/^-?(?:\d+(?:\.\d*)?|\.\d+)$/.test(s)){
  const negative=s.startsWith('-');const [whole='',part='']=s.replace(/^-/,'').split('.');
  const integer=whole.replace(/^0+/,'')||'0',fraction=part.replace(/0+$/,'');
  return (negative&&(integer!=='0'||fraction)?'-':'')+integer+(fraction?'.'+fraction:'');
 }
 if(field==='Time')return sourceTime(s)??s;return s;
}
export function parseEbird(text:string):Preview{
 const p:Preview={visits:[],rows:[],issues:[],unsupported:false,places:0,unspecified:0,unmatched:[],media:0};
 let records:ReturnType<typeof csvRecords>;
 try{records=csvRecords(text);}catch(e){const x=e as {line:number;code:'csv'|'limit'};p.issues.push({line:x.line??1,field:'',code:x.code??'csv'});return p;}
 const header=records.shift()?.values??[];
 if(header.includes('Species')&&header.includes('Observation Date')&&!header.includes('Submission ID')){p.unsupported=true;return p;}
 for(const field of required)if(!header.includes(field))p.issues.push({line:1,field,code:'header'});
 if(new Set(header).size!==header.length||header.some(x=>!x))p.issues.push({line:1,field:'',code:'header'});
 if(p.issues.length)return p;
 const visits=new Map<string,Visit>(),places=new Set<string>(),taxa=new Set<string>();
 for(const record of records){
  const {values,line}=record;const fields:Record<string,string>=Object.create(null);
  const issue=(field:string,code:Issue['code'])=>p.issues.push({line,field,code});
  if(values.length>header.length||values.some(value=>value.includes('\0')))issue('','csv');
  header.forEach((field,i)=>{fields[field]=values[i]??'';if(i>=values.length&&!optional.has(field))issue(field,'required');});
  for(const field of required.filter(x=>x!=='Time'))if(!fields[field].trim())issue(field,'required');
  if(!/^S[1-9]\d*$/.test(fields['Submission ID']))issue('Submission ID','required');
  if(!/^L[1-9]\d*$/.test(fields['Location ID']))issue('Location ID','required');
  if(!dateValid(fields.Date))issue('Date','date');
  if(sourceTime(fields.Time)===null)issue('Time','time');
  for(const field of ['Latitude','Longitude']){const s=fields[field];if(!/^-?(?:\d+(?:\.\d*)?|\.\d+)$/.test(s)||!Number.isFinite(Number(s))||Math.abs(Number(s))>(field==='Latitude'?90:180))issue(field,'coordinate');}
  const raw=fields.Count;let count:string|null=null;
  if(raw!=='X'){if(!/^[1-9]\d*$/.test(raw)||BigInt(raw)>9223372036854775807n)issue('Count','count');else count=raw;}else p.unspecified++;
  for(const field of ['Duration (Min)','Distance Traveled (km)','Area Covered (ha)','Taxonomic Order'])if(fields[field]&&!number(fields[field]))issue(field,'number');
  if(fields['Number of Observers']&&(!/^[1-9]\d*$/.test(fields['Number of Observers'])||!Number.isSafeInteger(Number(fields['Number of Observers']))))issue('Number of Observers','number');
  if(fields['All Obs Reported']&&!['0','1'].includes(fields['All Obs Reported']))issue('All Obs Reported','number');
  const media=(fields['ML Catalog Numbers']??'').trim().split(/\s+/).filter(Boolean);if(media.some(id=>!mediaId(id)))issue('ML Catalog Numbers','media');
  const row:SourceRow={line,fields,count,media};p.rows.push(row);p.media+=media.length;
  if(fields['Location ID'])places.add(fields['Location ID']);if(fields['Scientific Name'])taxa.add(fields['Scientific Name']);
  const id=fields['Submission ID'];if(!id)continue;
  let visit=visits.get(id);
  if(!visit){visit={id,fields,rows:[]};visits.set(id,visit);}
  else for(const field of metadata)if(comparable(field,visit.fields[field]??'')!==comparable(field,fields[field]??''))issue(field,'metadata');
  if(visit.rows.some(r=>r.fields['Scientific Name'].toLocaleLowerCase()===fields['Scientific Name'].toLocaleLowerCase()))issue('Scientific Name','duplicate');
  visit.rows.push(row);
 }
 if(!records.length)p.issues.push({line:2,field:'',code:'required'});
 p.visits=[...visits.values()];p.places=places.size;p.unmatched=[...taxa];return p;
}
export function applyCatalogMatches(p:Preview,matches:Species[][],names:string[]):Preview{
 const byName=new Map(names.map((name,i)=>[name,(matches[i]??[]).filter(s=>s.scientific.toLocaleLowerCase()===name.toLocaleLowerCase())]));
 const rows=p.rows.map(row=>{const candidates=byName.get(row.fields['Scientific Name'])??[];return {...row,catalogMatch:candidates.length===1?candidates[0]:undefined};});
 const byLine=new Map(rows.map(r=>[r.line,r]));
 return {...p,rows,visits:p.visits.map(v=>({...v,rows:v.rows.map(r=>byLine.get(r.line)!)})),unmatched:[...new Set(rows.filter(r=>!r.catalogMatch).map(r=>r.fields['Scientific Name']).filter(Boolean))]};
}
export function mediaId(id:string):boolean{return /^[1-9]\d*$/.test(id);}
export function mediaHref(id:string):string{if(!mediaId(id))throw Error('invalid media ID');return `https://macaulaylibrary.org/asset/${id}`;}
