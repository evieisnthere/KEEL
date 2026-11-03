import sys,re,collections
BIN=sys.argv[1]; sys.argv=['x',BIN,'sw_xref.pkl']; exec(open(__file__.replace('shiftscan.py','swdis.py')).read())
md.skipdata=True
core=[0x1c,0x20,0x24,0x28,0x2c,0x30]; extra=[0x34,0x78,0x7c,0x80,0x84,0x88,0x90,0x94,0x98,0x6c]
out=[]
for s,e in funcs:
    if e-s>0x30000: continue
    seen=collections.defaultdict(set)
    for ins in md.disasm(rd(s,e-s),s):
        if 'ss' not in ins.mnemonic: continue
        m=re.search(r'dword ptr \[(r\w+) \+ 0x([0-9a-f]+)\]',ins.op_str)
        if m and m.group(1) not in('rsp','rip'): seen[m.group(1)].add(int(m.group(2),16))
    for r,D in seen.items():
        for K in {d-0x1c for d in D}:
            if all(K+c in D for c in core):
                x=sum(K+c in D for c in extra)
                if x>=3: out.append((x,hex(s),e-s,r,hex(K),sorted(hex(c) for c in extra if K+c in D)))
for o in sorted(out,reverse=True)[:30]: print(o)
