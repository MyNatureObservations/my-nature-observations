import {invoke} from "@tauri-apps/api/core";
export const birdModule = $state({revision:0, changing:false});
export type DisableRequest={waitUntil:(result:Promise<boolean>)=>void};
export async function approveBirdDisable():Promise<boolean>{
 const pending:Promise<boolean>[]=[];
 window.dispatchEvent(new CustomEvent<DisableRequest>("mno:before-bird-disable",{detail:{waitUntil:result=>pending.push(result)}}));
 return (await Promise.all(pending)).every(Boolean);
}
export function refreshBirdModuleViews():void{
 birdModule.revision++;
 window.dispatchEvent(new Event("mno:bird-module-changed"));
 window.dispatchEvent(new Event("mno:journal-changed"));
}
export async function setBirdModule(enabled:boolean):Promise<void>{
 await invoke("set_bird_module_enabled",{enabled});
 refreshBirdModuleViews();
}
