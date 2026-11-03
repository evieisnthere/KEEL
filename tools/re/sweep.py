# full capstone sweep per function: record every rip-relative target -> [insn addrs]
import sys, pickle, collections, re
BIN,XR=sys.argv[1],sys.argv[2]; sys.argv=['x',BIN,XR]
exec(open(__file__.replace('sweep.py','swdis.py')).read())
md.skipdata=True
allx=collections.defaultdict(list)
for s,e in funcs:
    if e-s>0x40000: continue
    for ins in md.disasm(rd(s,e-s),s):
        m=re.search(r'rip ([+-]) 0x([0-9a-f]+)',ins.op_str)
        if m:
            t=ins.address+ins.size+(int(m.group(2),16)*(1 if m.group(1)=='+' else -1))
            allx[t].append(ins.address)
pickle.dump(dict(allx),open(sys.argv[2].replace('.pkl','_full.pkl'),'wb'))
print(len(allx))
