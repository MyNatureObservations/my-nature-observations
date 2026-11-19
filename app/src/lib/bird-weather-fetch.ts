import {invoke} from '@tauri-apps/api/core';
import {sameVisit,type VisitContext,type VisitWeather} from '$lib/bird-weather';
import type {WeatherPart} from '$lib/weather';
export type WeatherOwner={ownerId:string;expectedToken:string|null};
export type VisitRequest=WeatherOwner&{context:VisitContext};
export type VisitPreview={previewId:string;record:VisitWeather;todayForecast:boolean};
async function receive(command:string,generation:number,previewId:string,request:VisitRequest):Promise<VisitPreview>{
 const response=await invoke<{generation:number;data:VisitPreview}>(command,{generation,previewId,request});
 const p=response.data;
 if(response.generation!==generation||p.previewId!==previewId||p.record.list_id!==request.ownerId||p.record.kind!=='fetched'||p.record.stale||!sameVisit(JSON.parse(p.record.context_json),request.context))throw 'bird_weather_context_conflict';
 return p;
}
export const fetchVisitWeather=(generation:number,id:string,request:VisitRequest)=>receive('preview_bird_visit_weather',generation,id,request);
export const reviewVisitWeather=(generation:number,id:string,request:VisitRequest)=>receive('review_bird_visit_weather',generation,id,request);
export async function cancelVisitWeather(previewId:string):Promise<void>{await invoke('cancel_bird_visit_weather',{previewId});}
// Retain the meaning of "today" at retrieval, rather than changing the label on restart.
export function forecastOnRetrievalDay(part:WeatherPart):boolean{
 if(part.source?.product!=='forecast-daily'||part.period.kind!=='day'||!part.retrievedAt||!part.timezone)return false;
 try{
  const parts=new Intl.DateTimeFormat('en-CA',{timeZone:part.timezone,year:'numeric',month:'2-digit',day:'2-digit'}).formatToParts(new Date(part.retrievedAt));
  const field=(type:string)=>parts.find(p=>p.type===type)?.value;
  return `${field('year')}-${field('month')}-${field('day')}`===part.period.date;
 }catch{return false;}
}
