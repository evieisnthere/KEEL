import sys, pefile, struct, pickle, collections
pe=pefile.PE(sys.argv[1], fast_load=True); base=pe.OPTIONAL_HEADER.ImageBase
text=[s for s in pe.sections if s.Name.startswith(b'.text')][0]
data=text.get_data(); tva=base+text.VirtualAddress; end=tva+len(data)
cx=collections.defaultdict(list)
for i in range(len(data)-5):
    if data[i] in (0xE8,0xE9):
        t=tva+i+5+struct.unpack_from('<i',data,i+1)[0]
        if tva<=t<end: cx[t].append(tva+i)
# all rip-relative memory refs generically: scan for modrm with mod=00 rm=101 after common opcodes is hard; do capstone sweep per pdata func instead
pickle.dump(dict(cx),open(sys.argv[2],'wb'))
