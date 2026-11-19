import type {BirdPhoto} from "$lib/birds";
export function birdPhotoUrl(photo:BirdPhoto):string{return `data:${photo.mime};base64,${photo.preview}`;}
// The callback is draft-only. Invalidating an operation never writes or deletes a file.
export function birdPhotoTask(){let sequence=0,disposed=false;return {start(){const id=++sequence;return ()=>!disposed&&id===sequence;},cancel(){sequence++;},dispose(){disposed=true;sequence++;}};}
