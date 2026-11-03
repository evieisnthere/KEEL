//! Stormworks `.mesh` reader (format in docs/reverse-engineering/0002).

#[derive(Debug, Clone, PartialEq)]
pub struct Submesh {
    pub index_start: u32,
    pub index_count: u32,
    pub shader: u32,
    pub bb_min: [f32; 3],
    pub bb_max: [f32; 3],
    pub name: String,
    pub tint: [f32; 3],
}

#[derive(Debug, Clone, Default)]
pub struct Mesh {
    pub version: u16,
    pub positions: Vec<[f32; 3]>,
    pub colours: Vec<[u8; 4]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u16>,
    pub submeshes: Vec<Submesh>,
}

struct R<'a> {
    b: &'a [u8],
    o: usize,
}
impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let s = self.b.get(self.o..self.o + n).ok_or_else(|| format!("truncated at {}", self.o))?;
        self.o += n;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f32(&mut self) -> Result<f32, String> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn v3(&mut self) -> Result<[f32; 3], String> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
}

pub fn parse(b: &[u8]) -> Result<Mesh, String> {
    let mut r = R { b, o: 0 };
    if r.take(4)? != b"mesh" {
        return Err("bad magic".into());
    }
    let version = r.u16()?;
    let _a = r.u16()?;
    let nv = r.u16()? as usize;
    let _b = r.u16()?;
    let _c = r.u16()?;
    let mut m = Mesh { version, ..Default::default() };
    for _ in 0..nv {
        m.positions.push(r.v3()?);
        let c = r.take(4)?;
        m.colours.push([c[0], c[1], c[2], c[3]]);
        m.normals.push(r.v3()?);
    }
    let ni = r.u32()? as usize;
    for _ in 0..ni {
        let i = r.u16()?;
        if i as usize >= nv {
            return Err(format!("index {i} out of range ({nv} vertices)"));
        }
        m.indices.push(i);
    }
    let ns = r.u16()?;
    for _ in 0..ns {
        let index_start = r.u32()?;
        let index_count = r.u32()?;
        let shader = r.u32()?;
        let bb_min = r.v3()?;
        let bb_max = r.v3()?;
        let _u = r.u16()?;
        let nlen = r.u16()? as usize;
        let name = String::from_utf8_lossy(r.take(nlen)?).into_owned();
        let tint = r.v3()?;
        m.submeshes.push(Submesh { index_start, index_count, shader, bb_min, bb_max, name, tint });
    }
    Ok(m)
}
