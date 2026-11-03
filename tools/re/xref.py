import sys, pefile, struct, pickle, collections
path=sys.argv[1]; out=sys.argv[2]
pe=pefile.PE(path, fast_load=True)
base=pe.OPTIONAL_HEADER.ImageBase
text=[s for s in pe.sections if s.Name.startswith(b'.text')][0]
data=text.get_data(); tva=base+text.VirtualAddress
xr=collections.defaultdict(list)
n=len(data)
for i in range(n-7):
    b0=data[i]
    if b0 in (0x48,0x4c) and data[i+1]==0x8d and (data[i+2]&0xC7)==0x05:
        d=struct.unpack_from('<i',data,i+3)[0]; xr[tva+i+7+d].append(tva+i)
    # movss/movsd xmm,[rip+d]: F3 0F 10 modrm / F2 0F 10, optionally with REX
    if b0 in (0xF3,0xF2) and data[i+1]==0x0F and data[i+2] in (0x10,0x59,0x58,0x5C,0x5E,0x2F,0x5D,0x5F) and (data[i+3]&0xC7)==0x05:
        d=struct.unpack_from('<i',data,i+4)[0]; xr[tva+i+8+d].append(tva+i)
pickle.dump(dict(xr),open(out,'wb'))
print(len(xr))
