import sys, angr, logging
logging.getLogger('angr').setLevel(logging.ERROR); logging.getLogger('cle').setLevel(logging.ERROR); logging.getLogger('pyvex').setLevel(logging.ERROR)
binp=sys.argv[1]; addrs=[int(a,16) for a in sys.argv[2:]]
p=angr.Project(binp, auto_load_libs=False, load_options={'main_opts':{'base_addr':0x140000000}})
for a in addrs:
    cfg=p.analyses.CFGFast(function_starts=[a], normalize=True, force_complete_scan=False, symbols=False, function_prologues=False, start_at_entry=False, regions=[(a,a+0x8000)])
    f=cfg.kb.functions.get(a)
    try:
        d=p.analyses.Decompiler(f, cfg=cfg.model)
        print(f"// ==== {a:#x}\n"+ (d.codegen.text if d.codegen else "no codegen"))
    except Exception as e: print("ERR",a,e)
