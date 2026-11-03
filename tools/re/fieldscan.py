import sys,re,collections
BIN=sys.argv[1]; want=[int(x,16) for x in sys.argv[2].split(',')]; minhit=int(sys.argv[3])
sys.argv=['x',BIN,'sw_xref.pkl']; exec(open(__file__.replace('fieldscan.py','swdis.py')).read())
md.skipdata=True
res=[]
for s,e in funcs:
    if e-s>0x20000: continue
    seen=collections.defaultdict(set)
    for ins in md.disasm(rd(s,e-s),s):
        if ins.mnemonic in('movss','mulss','addss','subss','comiss','maxss','minss','divss'):
            m=re.search(r'dword ptr \[(r\w+) \+ 0x([0-9a-f]+)\]',ins.op_str)
            if m and m.group(1) not in('rsp','rbp','rip'):
                seen[m.group(1)].add(int(m.group(2),16))
    for r,ds in seen.items():
        h=sum(w in ds for w in want)
        if h>=minhit: res.append((h,s,e,r))
for h,s,e,r in sorted(res,reverse=True)[:25]: print(h,hex(s),hex(e),e-s,r)
