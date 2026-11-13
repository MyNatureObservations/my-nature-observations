import type { Species } from "$lib/birds";
export type BirdNamePreferences={showOtherLanguage:boolean;showScientific:boolean};
export const DEFAULT_BIRD_NAMES:BirdNamePreferences={showOtherLanguage:false,showScientific:false};
const KEY="mno:bird-name-preferences:v1";
type Store=Pick<Storage,"getItem"|"setItem">;
export function loadBirdNamePreferences(storage?:Store):BirdNamePreferences{
 try{const p=JSON.parse((storage??window.localStorage).getItem(KEY)??"null");if(p&&typeof p.showOtherLanguage==="boolean"&&typeof p.showScientific==="boolean")return {showOtherLanguage:p.showOtherLanguage,showScientific:p.showScientific};}catch{}
 return {...DEFAULT_BIRD_NAMES};
}
export function saveBirdNamePreferences(p:BirdNamePreferences,storage?:Store):boolean{
 try{(storage??window.localStorage).setItem(KEY,JSON.stringify(p));return true;}catch{return false;}
}
export function capitalizeCommonName(value:string,locale:"sv"|"en"):string{
 if(value.startsWith("avibase-"))return value;
 const [first,...rest]=Array.from(value);return first?first.toLocaleUpperCase(locale)+rest.join(""):value;
}
export function formatBirdDate(value:string,locale:"sv"|"en"):string{
 const date=new Date(`${value}T12:00:00Z`);
 if(!/^\d{4}-\d{2}-\d{2}$/.test(value)||!Number.isFinite(date.getTime())||date.toISOString().slice(0,10)!==value)return value;
 return new Intl.DateTimeFormat(locale==="sv"?"sv-SE":"en-GB",{day:"numeric",month:"long",year:"numeric",timeZone:"UTC"}).format(date);
}
export function formatBirdCount(value:string,locale:"sv"|"en"):string{
 return new Intl.NumberFormat(locale==="sv"?"sv-SE":"en-GB").format(BigInt(value));
}
export function displayBirdNames(s:Pick<Species,"english"|"swedish"|"scientific">,locale:"sv"|"en",p:BirdNamePreferences){
 const missing=locale==="sv"&&!s.swedish;
 return {primary:capitalizeCommonName(locale==="sv"?(s.swedish??s.english):s.english,locale),
 other:p.showOtherLanguage?(locale==="sv"?(!missing?capitalizeCommonName(s.english,locale):null):(s.swedish?capitalizeCommonName(s.swedish,locale):null)):null,
 scientific:p.showScientific||missing?s.scientific:null,
 missingSwedish:missing||(locale==="en"&&p.showOtherLanguage&&!s.swedish)};
}
