"""Convert the owner-approved AviList Explorer export. No network access."""
import csv, hashlib, json, pathlib, sys
EXPECTED='be9df523541c42736f21696be902eb9adabfcc4b9f4501c2562c88eac4d81ed5'
p=pathlib.Path(sys.argv[1]);raw=p.read_bytes()
if hashlib.sha256(raw).hexdigest()!=EXPECTED: raise SystemExit('Unreviewed source CSV checksum')
rows=[]
with p.open(encoding='utf-8-sig',newline='') as f:
 for r in csv.DictReader(f):
  if r['RANK']=='species': rows.append(dict(id=r['AVIBASE ID'],english=r['ENGLISH NAME: AVILIST'],swedish=r['svenska'] or None,scientific=r['SCIENTIFIC NAME']))
assert len(rows)==11172 and len({r['id'] for r in rows})==11172
assert all(r['english'] and r['scientific'] for r in rows)
assert sum(r['swedish'] is None for r in rows)==28
out=pathlib.Path(__file__).resolve().parents[2]/'src-tauri/catalog/avilist-v2026.json'
out.write_text(json.dumps(rows,ensure_ascii=False,separators=(',',':'))+'\n')
print(f'{len(rows)} species; 28 Swedish names absent; {out}')
