import sys, pefile, pickle, capstone, struct
pe=pefile.PE(sys.argv[1], fast_load=True); base=pe.OPTIONAL_HEADER.ImageBase
xr=pickle.load(open(sys.argv[2],'rb'))
md=capstone.Cs(capstone.CS_ARCH_X86, capstone.CS_MODE_64); md.detail=False
img=pe.get_memory_mapped_image()
def rd(va,n): return img[va-base:va-base+n]
def cstr(va):
    b=rd(va,200); return b.split(b'\0')[0].decode('latin1')
def f32(va): return struct.unpack('<f',rd(va,4))[0]
def f64(va): return struct.unpack('<d',rd(va,8))[0]
def dis(va,before=0x40,after=0x60):
    code=rd(va-before,before+after)
    for ins in md.disasm(code,va-before):
        s=f"{ins.address:x} {ins.mnemonic} {ins.op_str}"
        if 'rip' in ins.op_str:
            import re
            m=re.search(r'rip ([+-]) 0x([0-9a-f]+)',ins.op_str)
            if m:
                t=ins.address+ins.size+(int(m.group(2),16)*(1 if m.group(1)=='+' else -1))
                if 'xmm' in ins.op_str or 'dword' in ins.op_str:
                    s+=f"   ; [{t:x}] f32={f32(t):g}"
                elif 'qword' in ins.op_str: s+=f"   ; [{t:x}] f64={f64(t):g}"
                else:
                    c=cstr(t)
                    s+=f"   ; {t:x} '{c[:60]}'" if c.isprintable() and len(c)>2 else f"   ; {t:x}"
        print(s)
import bisect
pd=[s for s in pe.sections if s.Name.startswith(b'.pdata')][0].get_data()
funcs=sorted({(struct.unpack_from('<III',pd,i)[0]+base, struct.unpack_from('<III',pd,i)[1]+base) for i in range(0,len(pd)-12,12) if struct.unpack_from('<I',pd,i)[0]})
fstarts=[f[0] for f in funcs]
def func_of(va):
    i=bisect.bisect_right(fstarts,va)-1; return funcs[i]
def disfunc(va, maxlen=0x3000, lo=None, hi=None):
    s,e=func_of(va); e=min(e,s+maxlen)
    lo = lo or s; hi = hi or e
    code=rd(s,e-s); out=[]
    for ins in md.disasm(code,s):
        if ins.address<lo or ins.address>hi: continue
        dis_one(ins)
import re
def dis_one(ins):
    s=f"{ins.address:x} {ins.mnemonic} {ins.op_str}"
    m=re.search(r'rip ([+-]) 0x([0-9a-f]+)',ins.op_str)
    if m:
        t=ins.address+ins.size+(int(m.group(2),16)*(1 if m.group(1)=='+' else -1))
        if 'qword ptr' in ins.op_str and ('sd' in ins.mnemonic): s+=f"   ; f64={f64(t):g}"
        elif 'xmm' in ins.op_str or 'dword ptr' in ins.op_str: s+=f"   ; [{t:x}] f32={f32(t):g}"
        else:
            c=cstr(t)
            s+=f"   ; '{c[:70]}'" if c.isprintable() and len(c)>2 else f"   ; {t:x}"
    print(s)
