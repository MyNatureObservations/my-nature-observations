import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { capturePhotoPosition, insertPhotograph, type PhotographAttrs } from "../../src/lib/journal-photographs";
import { NodeSelection } from "@tiptap/pm/state";
import { mount, unmount, tick } from "svelte";
import JournalContent from "../../src/lib/components/JournalContent.svelte";
import { JournalImage } from "../../src/lib/journal-image";
import type { PhotographLabels } from "../../src/lib/photograph-controls";
import "../../src/lib/journal-photographs.css";
import en from "../../messages/en.json";
import sv from "../../messages/sv.json";

const host = document.querySelector<HTMLElement>("#editor")!, reader = document.querySelector<HTMLElement>("#reader")!;
const selectLocale = document.querySelector<HTMLSelectElement>("#locale")!, selectWidth = document.querySelector<HTMLSelectElement>("#width")!;
const canvas = document.createElement("canvas");
const assets = new Map();
const dimensions = [[900,600],[600,900],[80,40]];
for (let i=0;i<3;i++) {
  const [width,height]=dimensions[i];canvas.width=width;canvas.height=height;
  const context=canvas.getContext("2d")!;context.fillStyle=["#9ba88e","#b5ab98","#839d99"][i];context.fillRect(0,0,width,height);
  assets.set(`b7d05577-9248-49b7-af7b-2a240ee52bd${i}`,{asset_id:`b7d05577-9248-49b7-af7b-2a240ee52bd${i}`,entry_id:"entry",original_path:"/missing/synthetic.png",mime:"image/png",width,height,preview:canvas.toDataURL().split(",")[1]});
}
// Only the reader's data bridge is doubled. Both rendering implementations are real.
const nativeInvoke = (window as any).__TAURI_INTERNALS__?.invoke;
(window as any).__photoFixtureAssets = assets;
let sequence=0;
const prose="A quiet observation beside the photograph. The journal keeps its original order and leaves space for the landscape and its caption. ".repeat(12);
const paragraph=()=>({type:"paragraph",content:[{type:"text",text:prose}]});
const photo=(placement:PhotographAttrs["placement"],size:PhotographAttrs["size"],index=0)=>({type:"journalImage",attrs:{nodeId:`d581b905-b692-498d-8955-${String(++sequence).padStart(12,"0")}`,assetId:[...assets.keys()][index],placement,size,caption:sequence===2?"A long caption with an unbroken word: "+"landscape".repeat(45):"A calm caption",alt:"Synthetic photograph"}});
const content:any[]=[{type:"heading",attrs:{level:2},content:[{type:"text",text:"An editorial entry"}]}];
for(const placement of ["left","right","row"] as const) for(const size of ["small","medium","large"] as const) content.push(photo(placement,size,sequence%2),paragraph());
content.push(photo("row","large"),photo("left","small"),photo("right","large",1),{type:"heading",attrs:{level:2},content:[{type:"text",text:"A structural boundary"}]},photo("left","large"),{type:"bulletList",content:[{type:"listItem",content:[{type:"paragraph",content:[{type:"text",text:"A list clears the photograph"}]}]}]},photo("right","large",2),paragraph(),{type:"paragraph"});
// Explicit centered coverage: both orientations and tiny natural widths, at both sizes.
for (const size of ["small", "medium", "large"] as const) for (const index of [0,1,2]) content.push(photo("row",size,index),paragraph());
const doc={type:"doc",content};
function labels():PhotographLabels {const source:any=selectLocale.value==="sv"?sv:en;return Object.fromEntries(["photograph","loading","unavailable","edit","caption","alt","remove","done","cancel","left","right","row","small","medium","large","placement","size","changed","tooLong"].map(key=>[key,source[`photo_${key==="tooLong"?"too_long":key}`]])) as PhotographLabels;}
const editor=new Editor({element:host,extensions:[StarterKit,JournalImage.configure({labels,loadAsset:async id=>assets.get(id)})],content:doc});
let component:ReturnType<typeof mount>|undefined;
async function renderReader(){if(component)await unmount(component);component=mount(JournalContent,{target:reader,props:{contentJson:JSON.stringify(editor.getJSON()),documentVersion:2,entryId:"entry",generation:1,locale:selectLocale.value as "en"|"sv"}});await tick();}
async function settle(){await tick();await new Promise(resolve=>setTimeout(resolve,100));await Promise.all([...document.images].map(img=>img.decode().catch(()=>{})));}
function setWidth(width:number){host.style.width=reader.style.width=`${width}px`;}
async function run(){
  editor.commands.setContent(doc);
  await renderReader();await settle();const results:any[]=[];const failures:string[]=[];
  const check=(condition:boolean,message:string)=>{if(!condition)failures.push(message);};
  const saved=JSON.stringify(editor.getJSON());
  for(const width of [1000,760,640,639,360]) {
    setWidth(width);await settle();
    const editRoot=host.querySelector<HTMLElement>(".tiptap")!,readRoot=reader.querySelector<HTMLElement>(".journal-text")!;
    const editFigures=[...editRoot.querySelectorAll<HTMLElement>("figure")],readFigures=[...readRoot.querySelectorAll<HTMLElement>("figure")];
    const originE=editRoot.getBoundingClientRect(),originR=readRoot.getBoundingClientRect();
    check(editFigures.length===readFigures.length,`figure count at ${width}`);
    for(let i=0;i<editFigures.length;i++) {
      const figure=editFigures[i],other=readFigures[i],rect=figure.getBoundingClientRect(),readRect=other.getBoundingClientRect(),img=figure.querySelector("img")!;
      check(Math.abs(rect.width-readRect.width)<1&&Math.abs(rect.height-readRect.height)<1&&Math.abs((rect.top-originE.top)-(readRect.top-originR.top))<1,`editor/reader geometry ${width}/${i}: deltaY=${(rect.top-originE.top)-(readRect.top-originR.top)}, deltaH=${rect.height-readRect.height}`);
      check(rect.width<=img.naturalWidth+1&&rect.right<=originE.right+1&&rect.left>=originE.left-1,`bounds/no upscale ${width}/${i}`);
      check(Math.abs(img.getBoundingClientRect().width/img.getBoundingClientRect().height-img.naturalWidth/img.naturalHeight)<0.01,`aspect ratio ${width}/${i}`);
      check(rect.bottom<=originE.bottom+1,`float containment ${width}/${i}`);
      if(figure.dataset.placement==="row") {
        check(Math.abs((rect.left+rect.right)-(originE.left+originE.right))<1,`centered figure ${width}/${i}`);
        check(Math.abs((readRect.left+readRect.right)-(originR.left+originR.right))<1,`centered reader ${width}/${i}`);
        check(getComputedStyle(figure.querySelector("figcaption")!).textAlign==="center",`centered caption ${width}/${i}`);
        const paragraph=figure.nextElementSibling;
        if(paragraph?.tagName==="P")check(paragraph.getBoundingClientRect().top>=rect.bottom-1,`centered figure has no wrapping ${width}/${i}`);
      }
      const side=figure.dataset.placement!=="row"&&width>=640;
      const legacySide=figure.dataset.placement!=="row"&&figure.dataset.size==="large";
      const cap=figure.dataset.size==="small"?240:figure.dataset.size==="medium"||legacySide?360:640;
      const expected=Math.min(cap,img.naturalWidth,width,side?width*.45:Infinity);
      check(Math.abs(rect.width-expected)<1,`fixed size/natural limit ${width}/${i}: ${rect.width} vs ${expected}`);
      const floating=getComputedStyle(figure).float;
      check(floating===(width<640||figure.dataset.placement==="row"?"none":figure.dataset.placement),`responsive float ${width}/${i}`);
      const next=figure.nextElementSibling as HTMLElement|null;
      if(next?.matches("figure,h1,h2,h3,ul,ol"))check(next.getBoundingClientRect().top>=rect.bottom-1,`structural/consecutive clearance ${width}/${i}`);
    }
    const first=editFigures[0],p=first.nextElementSibling!;const range=document.createRange();range.selectNodeContents(p);const lines=[...range.getClientRects()];
    if(width>=640)check(lines.some(line=>line.left>=first.getBoundingClientRect().right+19&&line.top<first.getBoundingClientRect().bottom),`prose wraps ${width}`);
    results.push({width,figures:editFigures.length,firstWidth:first.getBoundingClientRect().width,mediumCenteredWidth:editRoot.querySelector('[data-placement="row"][data-size="medium"]')!.getBoundingClientRect().width,mediumSideWidth:editRoot.querySelector('[data-placement="left"][data-size="medium"]')!.getBoundingClientRect().width,legacyLargeSideWidth:editRoot.querySelector('[data-placement="left"][data-size="large"]')!.getBoundingClientRect().width});
  }
  editor.setEditable(false);
  check([...host.querySelectorAll<HTMLButtonElement>(".mno-photo-options")].every(button=>button.hidden),"readonly controls hidden");
  check([...host.querySelectorAll<HTMLElement>("figure")].every(figure=>figure.tabIndex===-1),"readonly figures have no editing tab stop");
  editor.setEditable(true);
  check(JSON.stringify(editor.getJSON())===saved,"resize/read mode does not mutate document");
  setWidth(Number(selectWidth.value));
  const clickTargets = checkClickTargets(check);
  const writing = await checkWriting(check);
  check(JSON.stringify(editor.getJSON())===saved,"click targets do not rewrite legacy layouts");
  const output={userAgent:navigator.userAgent,locale:selectLocale.value,results,clickTargets,writing,failures};
  document.querySelector("#results")!.textContent=JSON.stringify(output,null,2);
  (window as any).stage2b={editor,run,output,NodeSelection};
  await nativeInvoke?.("report",{value:JSON.stringify(output)});
  window.scrollTo(0,0);
}
selectWidth.onchange=()=>setWidth(Number(selectWidth.value));
selectLocale.onchange=async()=>{editor.storage.journalImage.controls?.close();editor.view.dispatch(editor.state.tr.setMeta("addToHistory",false));await run();};
document.querySelector<HTMLButtonElement>("#run")!.onclick=()=>void run();
void run();


function inspectWriting() {
  const root = host.querySelector<HTMLElement>(".tiptap")!, figure = root.querySelector("figure")!;
  const paragraph = figure?.nextElementSibling as HTMLElement | null;
  const range = document.createRange();
  if (paragraph) { range.selectNodeContents(paragraph); range.collapse(true); }
  const rect = (element: Element | null) => { const r = element?.getBoundingClientRect(); return r ? {left:r.left,top:r.top,right:r.right,bottom:r.bottom,height:r.height} : null; };
  const result = {activeElement:document.activeElement?.tagName,caretPosition:editor.view.coordsAtPos(editor.state.selection.from),selection:editor.state.selection.toJSON(),parent:editor.state.selection.$from.parent.type.name,paragraph:paragraph?.outerHTML,figure:rect(figure),paragraphRect:rect(paragraph),caret:[...range.getClientRects()].map(r=>({left:r.left,top:r.top,bottom:r.bottom})),json:editor.getJSON()};
  document.querySelector("#writing")!.textContent = JSON.stringify(result,null,2);
}
function insertForWriting(partial: boolean) {
  editor.commands.setContent({type:"doc",content:[{type:"paragraph",...(partial?{content:[{type:"text",text:"Earlier observation."}]}:{})}]});
  editor.commands.focus("end");
  const attrs = photo("row","large",partial?1:0).attrs;
  editor.view.dispatch(insertPhotograph(editor.state.tr,capturePhotoPosition(editor.state),attrs));
  editor.commands.focus();
  setTimeout(inspectWriting,100);
}
document.querySelector<HTMLButtonElement>("#insert-empty")!.onclick=()=>insertForWriting(false);
document.querySelector<HTMLButtonElement>("#insert-partial")!.onclick=()=>insertForWriting(true);
document.querySelector<HTMLButtonElement>("#inspect-writing")!.onclick=()=>{inspectWriting();void renderReader();};


async function checkWriting(check: (condition:boolean,message:string)=>void) {
  const results: object[]=[];
  for(const partial of [false,true]) for(const placement of ["left","right"] as const) for(const size of ["small","medium","large"] as const) {
    const writingHost=document.createElement("div");writingHost.style.width="760px";document.body.append(writingHost);
    const initial={type:"doc",content:[{type:"paragraph",...(partial?{content:[{type:"text",text:"Earlier observation."}]}:{})}]};
    const writer=new Editor({element:writingHost,extensions:[StarterKit,JournalImage.configure({labels,loadAsset:async id=>assets.get(id)})],content:initial});
    await settle();
    writer.commands.setTextSelection(writer.state.doc.content.size-1);
    const attrs=photo("row",size,placement==="left"?0:1).attrs;
    writer.view.dispatch(insertPhotograph(writer.state.tr,capturePhotoPosition(writer.state),attrs));
    await settle();
    let position=0;writer.state.doc.forEach((node,pos)=>{if(node.type.name==="journalImage")position=pos;});
    const figure=writingHost.querySelector<HTMLElement>("figure")!;
    writer.storage.journalImage.controls!.open({nodeId:attrs.nodeId,getPos:()=>position,element:figure});
    const choice=[...document.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]')].find(button=>button.textContent===labels()[placement])!;
    choice.click();
    check(writer.state.selection.$from.parent.type.name==="paragraph"&&writer.state.selection.from===position+2,`writing caret ${partial}/${placement}/${size}`);
    check(document.activeElement===writer.view.dom,`writing focus ${partial}/${placement}/${size}`);
    const expectedSize=size==="large"?"medium":size;
    check(writer.state.doc.nodeAt(position)?.attrs.size===expectedSize,`valid side size ${partial}/${placement}/${size}`);
    const beforeTyping=JSON.stringify(writer.getJSON());
    writer.commands.insertContent(prose);
    await settle();
    const typed=JSON.stringify(writer.getJSON()),paragraph=figure.nextElementSibling!;
    const range=document.createRange();range.selectNodeContents(paragraph);
    const figureRect=figure.getBoundingClientRect(),lines=[...range.getClientRects()];
    check(lines.some(line=>line.top<figureRect.bottom&&(placement==="left"?line.left>=figureRect.right+19:line.right<=figureRect.left-19)),`new prose wraps ${partial}/${placement}/${size}`);
    check(lines.some(line=>line.top>=figureRect.bottom),`new prose continues below ${partial}/${placement}/${size}`);
    check(writer.state.doc.nodeAt(position)?.attrs.nodeId===attrs.nodeId,`writing keeps image ${partial}/${placement}/${size}`);
    writer.commands.undo();check(JSON.stringify(writer.getJSON())===beforeTyping,`typing undo ${partial}/${placement}/${size}`);
    if(size==="large") {
      writer.commands.undo();check(writer.state.doc.nodeAt(position)?.attrs.placement==="row"&&writer.state.doc.nodeAt(position)?.attrs.size==="large",`atomic layout undo ${partial}/${placement}`);
      writer.commands.redo();check(JSON.stringify(writer.getJSON())===beforeTyping,`atomic layout redo ${partial}/${placement}`);
    }
    writer.commands.redo();check(JSON.stringify(writer.getJSON())===typed,`typing redo ${partial}/${placement}/${size}`);
    const readerHost=document.createElement("div");readerHost.style.width="760px";document.body.append(readerHost);
    const reading=mount(JournalContent,{target:readerHost,props:{contentJson:typed,documentVersion:2,entryId:"entry",generation:1,locale:selectLocale.value as "en"|"sv"}});
    await settle();
    const reopened=readerHost.querySelector("figure")!,readParagraph=reopened.nextElementSibling!;
    check(readParagraph.textContent===prose,`reopened prose ${partial}/${placement}/${size}`);
    check(Math.abs(reopened.getBoundingClientRect().width-figure.getBoundingClientRect().width)<1,`reopened figure width ${partial}/${placement}/${size}`);
    const readRange=document.createRange();readRange.selectNodeContents(readParagraph);
    const readLines=[...readRange.getClientRects()],readRect=reopened.getBoundingClientRect();
    check(lines.length===readLines.length&&Math.abs((lines[0].left-figureRect.left)-(readLines[0].left-readRect.left))<1,`writer/reader lines ${partial}/${placement}/${size}`);
    results.push({partial,placement,requestedSize:size,storedSize:writer.state.doc.nodeAt(position)?.attrs.size,caretBeforeTyping:position+2,keptImage:writer.state.doc.nodeAt(position)?.attrs.nodeId===attrs.nodeId});
    await unmount(reading);readerHost.remove();writer.destroy();writingHost.remove();
  }
  return results;
}


// Optional native input trace; records real pointer events, focus and transactions.
let tracingClicks = false;
const clickTrace: object[] = [];
function traceClick(type: string, event?: Event) {
  if (!tracingClicks) return;
  const target=event?.target as HTMLElement | undefined;
  const button=host.querySelector<HTMLButtonElement>(".ProseMirror-selectednode .mno-photo-options")??host.querySelector<HTMLButtonElement>(".mno-photo-options");
  const bounds=button?.getBoundingClientRect();
  const pointer=event instanceof MouseEvent?{clientX:event.clientX,clientY:event.clientY,screenX:event.screenX,screenY:event.screenY}:undefined;
  clickTrace.push({type,pointer,target:target?.tagName,className:target?.className,active:document.activeElement?.tagName,selection:editor.state.selection.toJSON(),menu:!!document.querySelector('[role="menu"]'),button:bounds?{left:bounds.left,top:bounds.top,width:bounds.width,height:bounds.height,opacity:getComputedStyle(button!).opacity,pointerEvents:getComputedStyle(button!).pointerEvents}:null});
  if(clickTrace.length>60)clickTrace.shift();
  setTimeout(()=>{document.querySelector("#click-log")!.textContent=JSON.stringify(clickTrace,null,2);void nativeInvoke?.("report",{value:JSON.stringify({kind:"click-trace",events:clickTrace})});},30);
}
for(const name of ["pointerdown","mousedown","focusin","pointerup","mouseup","click"]) document.addEventListener(name,event=>traceClick(name,event),true);
editor.on("transaction",()=>traceClick("transaction"));
document.querySelector<HTMLButtonElement>("#trace-clicks")!.onclick=()=>{clickTrace.length=0;tracingClicks=!tracingClicks;document.querySelector("#click-log")!.textContent=tracingClicks?"Recording native pointer/focus events":"Stopped";};


function checkClickTargets(check:(condition:boolean,message:string)=>void) {
  let points=0,buttons=0;
  const figures=[...host.querySelectorAll<HTMLElement>("figure")];
  for(const width of [760,360]) {
    setWidth(width);
    for(const figure of figures) {
      let pos=0;
      // Find the real NodeView position without adding attributes to saved content.
      editor.state.doc.forEach((node,offset)=>{if(editor.view.nodeDOM(offset)===figure)pos=offset;});
      editor.commands.setNodeSelection(pos);
      const button=figure.querySelector<HTMLButtonElement>("button")!;
      button.scrollIntoView({block:"center"});
      const rect=button.getBoundingClientRect();
      check(rect.width>=40&&rect.height>=40,`options target size ${width}/${pos}`);
      const textNode=button.firstChild;editor.view.dispatch(editor.state.tr.setMeta("addToHistory",false));
      check(button.firstChild===textNode,`stable button label ${width}/${pos}`);
      for(const [x,y] of [[rect.left+5,rect.top+5],[rect.right-5,rect.top+5],[rect.left+5,rect.bottom-5],[rect.right-5,rect.bottom-5],[rect.left+rect.width/2,rect.top+rect.height/2]]) {
        const hit=document.elementFromPoint(x,y);check(hit===button||button.contains(hit),`options hit area ${width}/${pos}/${points}`);points++;
      }
      const outside=document.elementFromPoint(rect.left-4,rect.top+rect.height/2);
      check(!!outside&&!outside.closest(".mno-photo-options"),`no invisible image overlay ${width}/${pos}`);
      buttons++;
    }
  }
  setWidth(Number(selectWidth.value));
  return {buttons,points};
}
