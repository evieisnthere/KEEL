import sys,re,pickle
BIN=sys.argv[1]; sys.argv=['x',BIN,'sw_xref.pkl']; exec(open(__file__.replace('allparams.py','swdis.py')).read())
md.skipdata=True
c=pickle.load(open('sw_calls.pkl','rb'))
SER={0x1408f3a70:'f32',0x1408f3350:'A',0x1408f35b0:'B',0x1408f30f0:'C',0x1408f70a0:'D'}
fs=sorted({func_of(a)[0] for k in SER for a in c.get(k,[])})
for s0 in fs:
    s,e=func_of(s0)
    regs={}; cur={}; rows=[]
    for ins in md.disasm(rd(s,e-s),s):
        op=ins.op_str
        m=re.search(r'rip ([+-]) 0x([0-9a-f]+)',op)
        t=ins.address+ins.size+(int(m.group(2),16)*(1 if m.group(1)=='+' else -1)) if m else None
        dst=op.split(',')[0].strip() if ',' in op else op
        if ins.mnemonic=='movss' and dst.startswith('xmm') and t: regs[dst]=round(f32(t),6)
        elif ins.mnemonic=='xorps' and len(set(x.strip() for x in op.split(',')))==1: regs[dst]=0.0
        elif ins.mnemonic=='mov' and t is None and re.match(r'^e?r?\w+$',dst) and re.match(r'^-?0x[0-9a-f]+$|^\d+$',op.split(',')[-1].strip()): regs[dst]=int(op.split(',')[-1].strip(),16 if 'x' in op.split(',')[-1] else 10)
        elif ins.mnemonic in('mov','movss') and op.startswith(('dword ptr [rsp + 0x20]','byte ptr [rsp + 0x20]','qword ptr [rsp + 0x20]')):
            v=op.split(',')[1].strip()
            try: cur['def']=int(v,16)
            except: cur['def']=regs.get(v, regs.get('e'+v[1:] if v.startswith('r') else v,'?'))
        elif ins.mnemonic=='lea' and dst=='r9': cur['off']=op.split('[')[1].rstrip(']')
        elif ins.mnemonic=='mov' and dst=='r9': cur['off']=op.split(',')[1].strip()
        elif ins.mnemonic=='lea' and dst=='r8' and t: cur['name']=cstr(t)
        elif ins.mnemonic=='call':
            try: tgt=int(op,16)
            except: tgt=None
            if tgt in SER and 'name' in cur: rows.append((SER[tgt],cur.get('name'),cur.get('off'),cur.get('def')))
            cur={}
    if rows:
        print(f"## func {s:#x}  ({len(rows)} fields)")
        for r in rows: print(f"  {r[0]:4s} {r[1]:40s} {str(r[2]):14s} {r[3]}")
