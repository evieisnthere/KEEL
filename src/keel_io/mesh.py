"""Reader for Stormworks `.mesh` files (format observed in v1.15.26 rom/meshes; E1 from file inspection).

Layout (little-endian):
  'mesh'  u16 version(=7)  u16 ?(=1)  u16 vertex_count  u16 ?(=0x13)  u16 ?(=0)
  vertex[vertex_count]: f32 pos[3], u8 rgba[4], f32 normal[3]                      (28 bytes)
  u32 index_count, u16 index[index_count]                                          (triangle list)
  u16 submesh_count
  submesh: u32 index_start, u32 index_count, u32 shader, f32 bb_min[3], f32 bb_max[3],
           u16 ?, u16 name_len, char name[name_len], f32 tint[3]
  u16 ? (trailer)
Coordinates are in metres in the component's local frame.
"""
from __future__ import annotations
import struct
from dataclasses import dataclass, field


class MeshError(ValueError):
    pass


@dataclass
class Submesh:
    index_start: int
    index_count: int
    shader: int
    bb_min: tuple
    bb_max: tuple
    name: str
    tint: tuple


@dataclass
class Mesh:
    version: int
    positions: list
    colours: list
    normals: list
    indices: list
    submeshes: list = field(default_factory=list)
    trailing: bytes = b''

    @property
    def triangles(self):
        ix = self.indices
        return [(ix[i], ix[i + 1], ix[i + 2]) for i in range(0, len(ix) - 2, 3)]


def parse(b: bytes) -> Mesh:
    if b[:4] != b'mesh':
        raise MeshError('bad magic')
    version, _a, nv, _b, _c = struct.unpack_from('<5H', b, 4)
    off = 14
    pos, col, nrm = [], [], []
    for _ in range(nv):
        x, y, z = struct.unpack_from('<3f', b, off); r, g, bl, a = b[off + 12:off + 16]
        nx, ny, nz = struct.unpack_from('<3f', b, off + 16)
        pos.append((x, y, z)); col.append((r, g, bl, a)); nrm.append((nx, ny, nz)); off += 28
    (ni,) = struct.unpack_from('<I', b, off); off += 4
    idx = list(struct.unpack_from('<%dH' % ni, b, off)); off += 2 * ni
    (ns,) = struct.unpack_from('<H', b, off); off += 2
    subs = []
    for _ in range(ns):
        start, count, shader = struct.unpack_from('<III', b, off); off += 12
        bb = struct.unpack_from('<6f', b, off); off += 24
        _u, nlen = struct.unpack_from('<HH', b, off); off += 4
        name = b[off:off + nlen].decode('latin1'); off += nlen
        tint = struct.unpack_from('<3f', b, off); off += 12
        subs.append(Submesh(start, count, shader, bb[:3], bb[3:], name, tint))
    if any(i >= nv for i in idx):
        raise MeshError('index out of range')
    return Mesh(version, pos, col, nrm, idx, subs, b[off:])


def load(path: str) -> Mesh:
    with open(path, 'rb') as f:
        return parse(f.read())
