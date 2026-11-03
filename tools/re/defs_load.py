import re,os,glob,xml.etree.ElementTree as ET
def load(D):
    defs={}; errs={}
    for p in sorted(glob.glob(D+'/*.xml')):
        s=open(p,'rb').read().decode('utf-8','replace')
        s=re.sub(r'(\s)(\d\w*)=',r'\1_\2=',s)   # attributes may start with a digit (game quirk)
        try: defs[os.path.basename(p)[:-4]]=ET.fromstring(s.encode())
        except Exception as e: errs[p]=str(e)
    return defs,errs
