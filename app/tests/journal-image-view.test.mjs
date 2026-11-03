import assert from "node:assert/strict";
import test from "node:test";
import { getSchema } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { EditorState, NodeSelection } from "@tiptap/pm/state";
import { history, undo, redo } from "@tiptap/pm/history";
import { JournalImage, imageViewUrl, controlsUrl } from "./photo-test-modules.mjs";
const { createPhotographView } = await import(imageViewUrl);
const { createPhotographControls } = await import(controlsUrl);
// Small DOM double for event and transaction behavior. WebKit verifies geometry separately.
class Element {
  constructor(tag) { this.tagName = tag; this.children = []; this.dataset = {}; this.value = ""; this.textContent = ""; this.hidden = false; this.attrs = {}; this.isConnected = true; this.classList = {add(){},remove(){}}; this.style = {setProperty(){},removeProperty(){}}; }
  append(...items) { this.children.push(...items); items.forEach(item => item.parentElement = this); }
  setAttribute(key,value) { this.attrs[key] = value; }
  removeAttribute(key) { delete this[key]; delete this.attrs[key]; }
  focus() { globalThis.document.activeElement = this; this.onfocus?.(); }
  closest() { return ["button","input","textarea","label"].includes(this.tagName) ? this : this.parentElement?.closest(); }
  remove() { this.isConnected = false; if(this.parentElement) this.parentElement.children=this.parentElement.children.filter(x=>x!==this); }
  contains(other) { return this === other || this.children.some(child=>child.contains(other)); }
  getBoundingClientRect() { return {left:20,top:20,width:200,height:200}; }
}
class Dialog extends Element { showModal(){this.open=true;} close(){this.open=false;} }
globalThis.Element = globalThis.HTMLElement = Element;
globalThis.HTMLDialogElement = Dialog;
const body = new Element("body"), events = new Map();
globalThis.document = { body, createElement: tag => tag === "dialog" ? new Dialog(tag) : new Element(tag), activeElement: null, addEventListener:(name,cb)=>events.set(name,cb),removeEventListener:name=>events.delete(name) };
globalThis.window = {innerWidth:1000,innerHeight:800};
const schema = getSchema([StarterKit, JournalImage]);
const attrs = {nodeId:"d581b905-b692-498d-8955-830251b82825",assetId:"b7d05577-9248-49b7-af7b-2a240ee52bd7",placement:"left",size:"small",caption:"Caption",alt:"Alternative"};
const asset = {asset_id:attrs.assetId,entry_id:"entry",original_path:"/missing/original.png",mime:"image/png",width:80,height:40,preview:"aGVsbG8="};
const labels = Object.fromEntries(["photograph","loading","unavailable","edit","caption","alt","remove","done","cancel","left","right","row","small","medium","large","placement","size","changed","tooLong"].map(key=>[key,key]));
function setup(loadAsset = async () => asset, following = "Prose", initialAttrs = attrs) {
  let state = EditorState.create({schema,doc:schema.nodeFromJSON({type:"doc",content:[{type:"journalImage",attrs:initialAttrs},{type:"paragraph",...(following ? {content:[{type:"text",text:following}]} : {})}]}),plugins:[history()]});
  const listeners = new Map(); let view, controls, position=0, generation=1, draft=false, changes=0;const documents=[];
  const editor = {get state(){return state;},isEditable:true,view:{focus(){},dispatch(tr){state=state.apply(tr);if(tr.docChanged){changes++;documents.push(state.doc.toJSON());} const node=state.doc.nodeAt(position);if(node?.type.name==="journalImage")view.update(node);for(const cb of listeners.get("transaction")??[])cb();}},commands:{focus(){},setNodeSelection(pos){editor.view.dispatch(state.tr.setSelection(NodeSelection.create(state.doc,pos)));}},on(name,callback){if(!listeners.has(name))listeners.set(name,new Set());listeners.get(name).add(callback);},off(name,cb){listeners.get(name)?.delete(cb);}};
  controls=createPhotographControls(editor,{labels:()=>labels,onDraft:v=>draft=v,session:()=>generation});
  view = createPhotographView({node:state.doc.firstChild,editor,getPos:()=>position},{loadAsset,labels:()=>labels,openControls:controls.open});
  const menu = () => body.children.at(-1);
  const items = () => menu().children.flatMap(x=>x.attrs.role==="group"?x.children:[x]);
  const choose = text => { const item=items().find(x=>x.textContent===text);assert.ok(item,text);item.onclick(); };
  const dialog = () => ({input:menu().children[0].children[1],error:menu().children[1],cancel:menu().children[2].children[0],apply:menu().children[2].children[1]});
  return {view,editor,controls,listeners,menu,items,choose,dialog,documents,get draft(){return draft;},get changes(){return changes;},move:v=>position=v,generation:v=>generation=v,destroy(){controls.destroy();view.destroy();}};
}
const key = (key,extra={}) => ({key,preventDefault(){this.prevented=true;},stopPropagation(){},...extra});

test("NodeView shares layout attributes, preserves preview and opens keyboard/right-click controls", async () => {
  const s=setup();await Promise.resolve();await Promise.resolve();
  const [img,status,caption,edit]=s.view.dom.children;
  assert.equal(img.src,"data:image/png;base64,aGVsbG8=");assert.equal(img.alt,"Alternative");assert.equal(status.hidden,true);
  assert.equal(caption.textContent,"Caption");assert.deepEqual(s.view.dom.dataset,{placement:"left",size:"small"});
  assert.equal(s.view.dom.children.length,4);assert.equal(edit.attrs["aria-haspopup"],"menu");
  s.view.dom.onkeydown(key("F10",{shiftKey:true}));assert.equal(s.menu().attrs.role,"menu");assert.equal(s.editor.state.selection instanceof NodeSelection,true);
  s.menu().onkeydown(key("End"));assert.equal(document.activeElement.textContent,"remove");s.menu().onkeydown(key("Home"));assert.equal(document.activeElement.textContent,"caption");s.menu().onkeydown(key("ArrowUp"));assert.equal(document.activeElement.textContent,"remove");
  s.menu().onkeydown(key("Escape"));assert.equal(document.activeElement,s.view.dom);
  s.view.dom.oncontextmenu({clientX:4,clientY:5,preventDefault(){},stopPropagation(){}});assert.equal(s.menu().attrs.role,"menu");
  s.editor.isEditable=false;for(const cb of s.listeners.get("update"))cb();assert.equal(edit.hidden,true);assert.equal(s.view.dom.tabIndex,-1);assert.equal(body.children.length,0);
  s.destroy();
});

test("draft Apply is one independent change; Cancel/no-op do not mutate; fields and identifiers stay separate", () => {
  const s=setup();const original=s.editor.state.doc.toJSON();
  s.view.dom.children[3].onclick();s.choose("caption");assert.equal(s.draft,true);s.dialog().input.value="Cancelled";assert.deepEqual(s.editor.state.doc.toJSON(),original);s.dialog().cancel.onclick();assert.equal(s.draft,false);assert.equal(s.changes,0);
  s.view.dom.children[3].onclick();s.choose("caption");s.dialog().input.value="Changed";s.dialog().apply.onclick();assert.equal(s.changes,1);assert.equal(s.editor.state.doc.firstChild.attrs.caption,"Changed");assert.equal(s.editor.state.doc.firstChild.attrs.alt,"Alternative");
  s.view.dom.children[3].onclick();s.choose("alt");s.dialog().input.value="New alt";s.dialog().apply.onclick();
  assert.equal(s.editor.state.doc.firstChild.attrs.nodeId,attrs.nodeId);assert.equal(s.editor.state.doc.firstChild.attrs.assetId,attrs.assetId);
  undo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.equal(s.editor.state.doc.firstChild.attrs.alt,"Alternative");assert.equal(s.editor.state.doc.firstChild.attrs.caption,"Changed");
  undo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.deepEqual(s.editor.state.doc.toJSON(),original);redo(s.editor.state,tr=>s.editor.view.dispatch(tr));
  const changes=s.changes;s.view.dom.children[3].onclick();s.choose("left");assert.equal(s.changes,changes);s.destroy();
});

test("every placement/size choice is undoable apart from neighboring prose edits", () => {
  const s=setup();
  s.editor.view.dispatch(s.editor.state.tr.insertText("Before",2));
  for(const placement of ["right","row","left"]) {s.view.dom.children[3].onclick();s.choose(placement);assert.equal(s.editor.state.doc.firstChild.attrs.placement,placement);}
  s.view.dom.children[3].onclick();s.choose("medium");s.editor.view.dispatch(s.editor.state.tr.insertText("After",2));
  undo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.equal(s.editor.state.doc.firstChild.attrs.size,"medium");assert.equal(s.editor.state.doc.lastChild.textContent,"BeforeProse");
  undo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.equal(s.editor.state.doc.firstChild.attrs.size,"small");
  for(const expected of ["row","right","left"]) {undo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.equal(s.editor.state.doc.firstChild.attrs.placement,expected);}
  assert.equal(s.editor.state.doc.lastChild.textContent,"BeforeProse");undo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.equal(s.editor.state.doc.lastChild.textContent,"Prose");s.destroy();
});

test("draft limits, external changes and stale targets cannot overwrite another node", () => {
  const s=setup();s.view.dom.children[3].onclick();s.choose("caption");s.dialog().input.value="x".repeat(4001);s.dialog().apply.onclick();assert.equal(s.dialog().error.textContent,"tooLong");assert.equal(s.changes,0);
  s.editor.view.dispatch(s.editor.state.tr.setNodeMarkup(0,undefined,{...attrs,caption:"External"}));s.dialog().input.value="stale";s.dialog().apply.onclick();assert.equal(s.dialog().error.textContent,"changed");assert.equal(s.editor.state.doc.firstChild.attrs.caption,"External");s.dialog().cancel.onclick();
  s.view.dom.children[3].onclick();s.choose("alt");const apply=s.dialog().apply;s.generation(2);apply.onclick();assert.equal(s.draft,false);assert.equal(s.editor.state.doc.firstChild.attrs.alt,"Alternative");
  s.view.dom.children[3].onclick();s.choose("alt");const stale=s.dialog().apply;s.editor.view.dispatch(s.editor.state.tr.delete(0,1));stale.onclick();assert.equal(s.editor.state.doc.firstChild.type.name,"paragraph");assert.equal(s.draft,false);s.destroy();
});

test("live positions follow document edits; removal retains normal undo", () => {
  const s=setup();s.view.dom.children[3].onclick();s.move(2);s.editor.view.dispatch(s.editor.state.tr.insert(0,schema.nodes.paragraph.create()));s.choose("right");assert.equal(s.editor.state.doc.nodeAt(2).attrs.placement,"right");
  s.view.dom.children[3].onclick();s.choose("remove");assert.equal(s.editor.state.doc.childCount,2);undo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.equal(s.editor.state.doc.nodeAt(2).attrs.assetId,attrs.assetId);s.destroy();
});

test("destroyed NodeView discards late preview bytes and failures stay localized", async () => {
  let resolve;const first=setup(()=>new Promise(done=>resolve=done));first.destroy();resolve(asset);await Promise.resolve();await Promise.resolve();assert.equal(first.view.dom.children[0].src,undefined);
  const second=setup(async()=>{throw new Error("missing")});await Promise.resolve();await Promise.resolve();await Promise.resolve();assert.equal(second.view.dom.children[1].textContent,labels.unavailable);second.destroy();
});


test("focused figure arrows return the caret to surrounding prose without document changes", () => {
  const s=setup();s.view.dom.focus();
  s.view.dom.onkeydown(key("ArrowRight",{target:s.view.dom}));
  assert.equal(s.editor.state.selection.$from.parent.type.name,"paragraph");assert.equal(s.changes,0);s.destroy();
});


for (const placement of ["left", "right", "row"]) test(`choosing ${placement} resumes the existing empty paragraph and preserves the image during typing`, () => {
  const s=setup(undefined, "");const initial=s.editor.state.doc.toJSON();
  s.view.dom.children[3].onclick();s.choose(placement);
  assert.equal(s.editor.state.selection.$from.parent.type.name,"paragraph");
  assert.equal(s.editor.state.selection.from,2);
  assert.equal(s.editor.state.doc.childCount,2);
  s.editor.view.dispatch(s.editor.state.tr.insertText("New observation."));
  assert.equal(s.editor.state.doc.firstChild.attrs.nodeId,attrs.nodeId);
  assert.equal(s.editor.state.doc.firstChild.attrs.assetId,attrs.assetId);
  assert.equal(s.editor.state.doc.lastChild.textContent,"New observation.");
  undo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.equal(s.editor.state.doc.lastChild.textContent,"");
  if(placement!=="left") {undo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.deepEqual(s.editor.state.doc.toJSON(),initial);redo(s.editor.state,tr=>s.editor.view.dispatch(tr));}
  redo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.equal(s.editor.state.doc.lastChild.textContent,"New observation.");s.destroy();
});

test("active placement and subsequent size choices keep an empty paragraph ready without extra document nodes", () => {
  const s=setup(undefined, "");
  s.view.dom.children[3].onclick();s.choose("left");assert.equal(s.changes,0);assert.equal(s.editor.state.selection.from,2);
  s.view.dom.children[3].onclick();s.choose("medium");assert.equal(s.changes,1);assert.equal(s.editor.state.selection.from,2);assert.equal(s.editor.state.doc.childCount,2);s.destroy();
});


test("Medium menu item participates in keyboard navigation, checked state, no-op and undo/redo", () => {
  const s=setup();s.view.dom.onkeydown(key("F10",{shiftKey:true}));
  const radio=s.items().find(item=>item.textContent==="medium");assert.equal(radio.attrs.role,"menuitemradio");assert.equal(radio.attrs["aria-checked"],"false");
  const small=s.items().find(item=>item.textContent==="small");small.focus();s.menu().onkeydown(key("ArrowDown"));assert.equal(document.activeElement,radio);
  document.activeElement.onclick();assert.equal(s.editor.state.doc.firstChild.attrs.size,"medium");
  s.view.dom.children[3].onclick();assert.equal(s.items().find(item=>item.textContent==="medium").attrs["aria-checked"],"true");
  const changes=s.changes;s.choose("medium");assert.equal(s.changes,changes);
  undo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.equal(s.editor.state.doc.firstChild.attrs.size,"small");
  redo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.equal(s.editor.state.doc.firstChild.attrs.size,"medium");
  assert.equal(s.editor.state.doc.firstChild.attrs.nodeId,attrs.nodeId);assert.equal(s.editor.state.doc.firstChild.attrs.assetId,attrs.assetId);s.destroy();
});


for(const placement of ["left","right"]) test(`Centered Large to ${placement} stores Medium atomically and undoes as one action`, () => {
  const s=setup(undefined,"",{...attrs,placement:"row",size:"large"});const original=s.editor.state.doc.toJSON();
  s.view.dom.children[3].onclick();assert.ok(s.items().some(item=>item.textContent==="large"));s.choose(placement);
  assert.equal(s.changes,1);assert.equal(s.documents.length,1);
  assert.equal(s.documents[0].content[0].attrs.placement,placement);assert.equal(s.documents[0].content[0].attrs.size,"medium");
  assert.equal(s.editor.state.doc.firstChild.attrs.nodeId,attrs.nodeId);assert.equal(s.editor.state.selection.from,2);
  s.view.dom.children[3].onclick();assert.ok(!s.items().some(item=>item.textContent==="large"));
  assert.equal(s.items().find(item=>item.textContent==="medium").attrs["aria-checked"],"true");s.controls.close();
  undo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.deepEqual(s.editor.state.doc.toJSON(),original);
  redo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.equal(s.editor.state.doc.firstChild.attrs.size,"medium");assert.equal(s.editor.state.doc.firstChild.attrs.placement,placement);s.destroy();
});

for(const placement of ["left","right","row"]) for(const size of ["small","medium","large"]) test(`size menu for stored ${placement}/${size} preserves load and offers valid choices`, () => {
  const s=setup(undefined,"Prose",{...attrs,placement,size});const original=s.editor.state.doc.toJSON();
  s.view.dom.children[3].onclick();
  const sizes=s.menu().children.find(group=>group.attrs["aria-label"]==="size").children;
  assert.deepEqual(sizes.map(item=>item.textContent),placement==="row"?["small","medium","large"]:["small","medium"]);
  assert.equal(sizes.find(item=>item.attrs["aria-checked"]==="true").textContent,placement!=="row"&&size==="large"?"medium":size);
  s.controls.close();assert.deepEqual(s.editor.state.doc.toJSON(),original);assert.equal(s.changes,0);
  if(placement!=="row"&&size==="large") {s.view.dom.children[3].onclick();s.choose("medium");assert.equal(s.editor.state.doc.firstChild.attrs.size,"medium");undo(s.editor.state,tr=>s.editor.view.dispatch(tr));assert.deepEqual(s.editor.state.doc.toJSON(),original);}
  s.destroy();
});

test("a stale Large menu action cannot introduce Large on a side placement", () => {
  const s=setup(undefined,"Prose",{...attrs,placement:"row",size:"medium"});s.view.dom.children[3].onclick();
  const large=s.items().find(item=>item.textContent==="large");
  s.editor.view.dispatch(s.editor.state.tr.setNodeMarkup(0,undefined,{...attrs,placement:"left",size:"medium"}));
  const changes=s.changes;large.onclick();assert.equal(s.changes,changes);assert.equal(s.editor.state.doc.firstChild.attrs.size,"medium");s.destroy();
});

test("selection refresh keeps the options label DOM stable; locale changes still update it", () => {
  const s=setup();const button=s.view.dom.children[3];let label=button.textContent,writes=0;
  Object.defineProperty(button,"textContent",{get:()=>label,set:value=>{label=value;writes++;}});
  s.view.dom.focus();s.editor.view.dispatch(s.editor.state.tr);assert.equal(writes,0);
  const old=labels.edit;labels.edit="Bildalternativ";s.editor.view.dispatch(s.editor.state.tr);assert.equal(writes,1);assert.equal(button.textContent,"Bildalternativ");labels.edit=old;s.destroy();
});
