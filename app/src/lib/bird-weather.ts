import type { WeatherPart, WeatherView } from '$lib/weather';
import type { Place } from '$lib/places';

export type VisitContext = {date:string;place_id:string;place_name:string;latitude:number;longitude:number;start:string|null;end:string|null};
export type VisitWeather = {list_id:string;kind:'manual'|'fetched';context_json:string;payload_json:string;stale:boolean};
export type WeatherChange = {action:'set';context:VisitContext;payload:WeatherPart;confirmed:boolean}|{action:'remove';kind:'manual'|'fetched'}|{action:'fetched';previewId:string};
export function visitContext(date:string,place:Place|null,start:string,end:string):VisitContext|null {
 if(!place||place.latitude===null||place.longitude===null)return null;
 return {date,place_id:place.id,place_name:place.name,latitude:place.latitude,longitude:place.longitude,start:start||null,end:end||null};
}
export function sameVisit(a:VisitContext|null,b:VisitContext|null):boolean {
 return !!a&&!!b&&(['date','place_id','place_name','latitude','longitude','start','end'] as const).every(key=>a[key]===b[key]);
}
export function weatherStale(record:VisitWeather,context:VisitContext|null):boolean {
 return record.stale||!sameVisit(JSON.parse(record.context_json),context);
}
export function visitWeatherSummary(records:VisitWeather[],context:VisitContext|null):WeatherView {
 const part=(kind:VisitWeather['kind'])=>{const r=records.find(r=>r.kind===kind&&!weatherStale(r,context));return r?JSON.parse(r.payload_json) as WeatherPart:null;};
 return {manual:part('manual'),fetched:part('fetched'),revision:'0'};
}
// Rebinding only occurs after the UI has explicitly confirmed the current context.
export function manualWeatherChange(part:WeatherPart,context:VisitContext,confirmed:boolean):WeatherChange {
 return {action:'set',context:{...context},confirmed,payload:{...part,period:{kind:'day',date:context.date},location:{name:context.place_name,coordinates:{latitude:context.latitude,longitude:context.longitude}}}};
}
