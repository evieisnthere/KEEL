import sys,re
BIN,XR,FN=sys.argv[1],sys.argv[2],int(sys.argv[3],16)
sys.argv=['x',BIN,XR]; exec(open(__file__.replace('params2.py','swdis.py')).read())
s,e=func_of(FN)
regs={}; stack={}; cur={}
rows=[]
for ins in md.disasm(rd(s,e-s),s):
    op=ins.op_str
    m=re.search(r'rip ([+-]) 0x([0-9a-f]+)',op)
    t=ins.address+ins.size+(int(m.group(2),16)*(1 if m.group(1)=='+' else -1)) if m else None
    if ins.mnemonic=='movss' and op.startswith('xmm') and t: regs[op.split(',')[0]]=f32(t)
    elif ins.mnemonic in('xorps',) and op.split(',')[0].strip()==op.split(',')[1].strip(): regs[op.split(',')[0]]=0.0
    elif ins.mnemonic=='movss' and op.startswith('dword ptr [rsp + 0x20]'):
        r=op.split(',')[1].strip(); cur['def']=regs.get(r,'?')
    elif ins.mnemonic=='mov' and op.startswith('dword ptr [rsp + 0x20]'):
        v=op.split(',')[1].strip()
        try: cur['def']=int(v,16)
        except: cur['def']=regs.get(v,'?')
    elif ins.mnemonic=='mov' and op.startswith('byte ptr [rsp + 0x20]'):
        cur['def']=op.split(',')[1].strip()
    elif ins.mnemonic=='lea' and op.startswith('r9,'):
        cur['off']=op.split('[')[1].rstrip(']')
    elif ins.mnemonic=='lea' and op.startswith('r8,') and t:
        cur['name']=cstr(t)
    elif ins.mnemonic=='call':
        if 'name' in cur: rows.append((cur.get('name'),cur.get('off'),cur.get('def'),op))
        cur={}
for r in rows: print(f"{r[0]:40s} {str(r[1]):12s} {r[2]!s:12s} {r[3]}")
