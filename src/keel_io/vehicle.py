"""Native Stormworks vehicle model (data_version 3) over the lossless XML layer.

Everything not modelled here is preserved untouched, because the model is a view onto the lossless tree.
Conventions (see docs/reverse-engineering/0002, 0004):
- Component position: ``o/vp`` integer voxel coordinates (0.25 m grid). A missing axis attribute means 0.
- Component rotation: ``o@r`` = 9 ints. Missing means identity. World voxel offset = r^T · v (column-major; E1-inferred).
- Missing ``c@d`` means the plain block ``01_block``.
- Logic links are stored by voxel position (``logic_node_links/logic_node_link/voxel_pos_0|1``).
"""
from __future__ import annotations
from dataclasses import dataclass
from typing import Iterator, Optional
from . import losslessxml as X

GRID_M = 0.25
DEFAULT_DEF = '01_block'
IDENTITY = (1, 0, 0, 0, 1, 0, 0, 0, 1)


def _vec_get(el: Optional[X.Element]) -> tuple:
    if el is None:
        return (0, 0, 0)
    return tuple(int(float(el.get(a, '0') or 0)) for a in 'xyz')


def _vec_set(el: X.Element, v: tuple) -> None:
    """Write a voxel vector the way the game does: omit zero axes."""
    for a, x in zip('xyz', v):
        if x == 0:
            el.remove_attr(a)
        else:
            el.set(a, int(x))


def rotate(r: tuple, v: tuple) -> tuple:
    return tuple(r[0 + i] * v[0] + r[3 + i] * v[1] + r[6 + i] * v[2] for i in range(3))


class Component:
    def __init__(self, c: X.Element):
        self.el = c
        self.o = c.find('o')

    @property
    def definition(self) -> str:
        return self.el.get('d') or DEFAULT_DEF

    @property
    def variant(self) -> Optional[str]:
        return self.el.get('t')

    @property
    def voxel(self) -> tuple:
        return _vec_get(self.o.find('vp')) if self.o is not None else (0, 0, 0)

    @voxel.setter
    def voxel(self, v: tuple) -> None:
        vp = self.o.find('vp')
        if vp is None:
            vp = X.Element('vp', self_closing=True); vp.mark_dirty(); self.o.append(vp)
        _vec_set(vp, tuple(int(x) for x in v))

    @property
    def rotation(self) -> tuple:
        r = self.o.get('r') if self.o is not None else None
        return tuple(int(x) for x in r.split(',')) if r else IDENTITY

    @rotation.setter
    def rotation(self, r: tuple) -> None:
        r = tuple(int(x) for x in r)
        if r == IDENTITY:
            self.o.remove_attr('r')
        else:
            self.o.set('r', ','.join(str(x) for x in r))

    @property
    def position_m(self) -> tuple:
        return tuple(x * GRID_M for x in self.voxel)

    def colours(self) -> dict:
        return {k: self.o.get(k) for k in ('bc', 'bc2', 'bc3', 'ac', 'sc') if self.o is not None and self.o.get(k) is not None}

    def __repr__(self):
        return f'<Component {self.definition} at {self.voxel}>'


class Body:
    def __init__(self, b: X.Element):
        self.el = b

    @property
    def unique_id(self) -> Optional[str]:
        return self.el.get('unique_id')

    def components(self) -> Iterator[Component]:
        cs = self.el.find('components')
        if cs is None:
            return iter(())
        return (Component(c) for c in cs.findall('c') if c.find('o') is not None)


@dataclass
class LogicLink:
    el: X.Element

    @property
    def type(self) -> int:
        return int(self.el.get('type', '0') or 0)

    @property
    def endpoints(self) -> tuple:
        return _vec_get(self.el.find('voxel_pos_0')), _vec_get(self.el.find('voxel_pos_1'))


class Vehicle:
    def __init__(self, doc: X.Document):
        if doc.root.tag != 'vehicle':
            raise X.XmlError(f'not a vehicle file (root <{doc.root.tag}>)')
        self.doc = doc

    @classmethod
    def load(cls, path: str) -> 'Vehicle':
        return cls(X.load(path))

    def save(self, path: str) -> None:
        X.save(self.doc, path)

    @property
    def data_version(self) -> Optional[str]:
        return self.doc.root.get('data_version')

    def bodies(self) -> list:
        bs = self.doc.root.find('bodies')
        return [Body(b) for b in bs.findall('body')] if bs is not None else []

    def components(self) -> Iterator[Component]:
        for b in self.bodies():
            yield from b.components()

    def logic_links(self) -> list:
        ll = self.doc.root.find('logic_node_links')
        return [LogicLink(e) for e in ll.findall('logic_node_link')] if ll is not None else []

    def authors(self) -> list:
        a = self.doc.root.find('authors')
        return [x.attrib for x in a.findall('author')] if a is not None else []
