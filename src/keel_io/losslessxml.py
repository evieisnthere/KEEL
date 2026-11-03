"""Lossless XML reader/writer for Stormworks files.

Design goals (PROJECT_PLAN §10.3):
- Byte-identical round trip for unmodified documents, including whitespace, attribute order, quoting, number formatting,
  and the game's non-standard attribute names that start with a digit (e.g. ``<physics_shape_rotation 00="1" .../>``).
- Edits are local: only elements whose tag/attributes were changed are re-serialised. Everything else is emitted from the
  original source span.
- No dependency on the game, Qt or a geometry kernel.

The parser is deliberately small and strict about structure (balanced tags) but permissive about names.
"""
from __future__ import annotations
import re
from dataclasses import dataclass, field
from typing import Iterator, Optional, Union

_NAME = r'[^\s=/<>"\']+'
_ATTR_RE = re.compile(r'(\s*)(' + _NAME + r')(\s*=\s*)("[^"]*"|\'[^\']*\')')
_TAG_RE = re.compile(r'<(/?)(' + _NAME + r')')
_ESC = {'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'}
_UNESC = {'&amp;': '&', '&lt;': '<', '&gt;': '>', '&quot;': '"', '&apos;': "'"}


class XmlError(ValueError):
    pass


def escape_attr(v: str) -> str:
    return ''.join(_ESC.get(c, c) for c in v)


def unescape(v: str) -> str:
    return re.sub(r'&(amp|lt|gt|quot|apos);', lambda m: _UNESC[m.group(0)], v)


@dataclass
class Attr:
    name: str
    raw: str              # quoted raw value as in source, e.g. '"1,0,0"'
    lead_ws: str = ' '    # whitespace before the name
    eq: str = '='         # the '=' including surrounding whitespace

    @property
    def value(self) -> str:
        return unescape(self.raw[1:-1])


@dataclass
class Element:
    tag: str
    attrs: list = field(default_factory=list)          # list[Attr]
    children: list = field(default_factory=list)       # list[Element | str]  (str = raw text/comments/PIs)
    self_closing: bool = False
    tag_tail_ws: str = ''                              # whitespace before '>' or '/>'
    _src_start: Optional[str] = None                   # raw start tag
    _src_end: Optional[str] = None                     # raw end tag ('' if self-closing)
    _dirty: bool = False

    # --- attribute API -------------------------------------------------------------------
    def get(self, name: str, default: Optional[str] = None) -> Optional[str]:
        for a in self.attrs:
            if a.name == name:
                return a.value
        return default

    def set(self, name: str, value) -> None:
        raw = '"' + escape_attr(str(value)) + '"'
        for a in self.attrs:
            if a.name == name:
                if a.raw != raw:
                    a.raw = raw; self._dirty = True
                return
        self.attrs.append(Attr(name, raw)); self._dirty = True

    def remove_attr(self, name: str) -> None:
        n = len(self.attrs)
        self.attrs = [a for a in self.attrs if a.name != name]
        if len(self.attrs) != n:
            self._dirty = True

    @property
    def attrib(self) -> dict:
        return {a.name: a.value for a in self.attrs}

    # --- tree API ------------------------------------------------------------------------
    def elements(self) -> Iterator['Element']:
        return (c for c in self.children if isinstance(c, Element))

    def find(self, tag: str) -> Optional['Element']:
        return next((c for c in self.elements() if c.tag == tag), None)

    def findall(self, tag: str) -> list:
        return [c for c in self.elements() if c.tag == tag]

    def iter(self, tag: Optional[str] = None) -> Iterator['Element']:
        if tag is None or self.tag == tag:
            yield self
        for c in self.elements():
            yield from c.iter(tag)

    def append(self, child: 'Element') -> None:
        if self.self_closing:
            self.self_closing = False; self._dirty = True
        self.children.append(child)
        if self._src_end is None or self._src_end == '':
            self._dirty = True

    def remove(self, child: 'Element') -> None:
        self.children.remove(child)

    def mark_dirty(self) -> None:
        self._dirty = True

    # --- serialisation -------------------------------------------------------------------
    def _start_tag(self) -> str:
        if not self._dirty and self._src_start is not None:
            return self._src_start
        parts = ['<', self.tag]
        for a in self.attrs:
            parts += [a.lead_ws or ' ', a.name, a.eq or '=', a.raw]
        parts.append(self.tag_tail_ws)
        parts.append('/>' if self.self_closing else '>')
        return ''.join(parts)

    def _end_tag(self) -> str:
        if self.self_closing:
            return ''
        if not self._dirty and self._src_end:
            return self._src_end
        return f'</{self.tag}>'

    def serialize(self, out: list) -> None:
        out.append(self._start_tag())
        if not self.self_closing:
            for c in self.children:
                if isinstance(c, Element):
                    c.serialize(out)
                else:
                    out.append(c)
            out.append(self._end_tag())


@dataclass
class Document:
    prolog: list            # raw strings before the root (XML decl, comments, whitespace)
    root: Element
    epilog: list            # raw strings after the root

    def to_string(self) -> str:
        out = list(self.prolog)
        self.root.serialize(out)
        out += self.epilog
        return ''.join(out)

    def to_bytes(self, encoding='utf-8') -> bytes:
        return self.to_string().encode(encoding)


def _parse_start(src: str, pos: int, name: str):
    """Parse attributes of a start tag beginning at src[pos] == '<'. Returns (element, end_pos)."""
    i = pos + 1 + len(name)
    attrs = []
    n = len(src)
    while True:
        m = _ATTR_RE.match(src, i)
        if not m:
            break
        attrs.append(Attr(m.group(2), m.group(4), m.group(1), m.group(3)))
        i = m.end()
    j = i
    while j < n and src[j] in ' \t\r\n':
        j += 1
    if src.startswith('/>', j):
        el = Element(name, attrs, self_closing=True, tag_tail_ws=src[i:j])
        el._src_start = src[pos:j + 2]; el._src_end = ''
        return el, j + 2
    if src.startswith('>', j):
        el = Element(name, attrs, tag_tail_ws=src[i:j])
        el._src_start = src[pos:j + 1]
        return el, j + 1
    raise XmlError(f'malformed start tag <{name} at offset {pos}: {src[pos:pos + 80]!r}')


def parse(data: Union[str, bytes]) -> Document:
    src = data.decode('utf-8') if isinstance(data, (bytes, bytearray)) else data
    n = len(src); i = 0
    prolog: list = []
    stack: list = []
    root: Optional[Element] = None
    epilog: list = []

    def emit_raw(s: str):
        if not s:
            return
        if stack:
            stack[-1].children.append(s)
        elif root is None:
            prolog.append(s)
        else:
            epilog.append(s)

    while i < n:
        lt = src.find('<', i)
        if lt < 0:
            emit_raw(src[i:]); break
        if lt > i:
            emit_raw(src[i:lt])
        if src.startswith('<!--', lt):
            e = src.find('-->', lt)
            if e < 0: raise XmlError('unterminated comment')
            emit_raw(src[lt:e + 3]); i = e + 3; continue
        if src.startswith('<?', lt):
            e = src.find('?>', lt)
            if e < 0: raise XmlError('unterminated processing instruction')
            emit_raw(src[lt:e + 2]); i = e + 2; continue
        if src.startswith('<![CDATA[', lt):
            e = src.find(']]>', lt)
            if e < 0: raise XmlError('unterminated CDATA')
            emit_raw(src[lt:e + 3]); i = e + 3; continue
        if src.startswith('<!', lt):
            e = src.find('>', lt)
            emit_raw(src[lt:e + 1]); i = e + 1; continue
        m = _TAG_RE.match(src, lt)
        if not m:
            raise XmlError(f'bad tag at offset {lt}: {src[lt:lt + 40]!r}')
        closing, name = m.group(1), m.group(2)
        if closing:
            e = src.find('>', lt)
            if not stack or stack[-1].tag != name:
                raise XmlError(f'mismatched </{name}> at offset {lt}')
            el = stack.pop(); el._src_end = src[lt:e + 1]
            i = e + 1
            continue
        el, i = _parse_start(src, lt, name)
        if stack:
            stack[-1].children.append(el)
        elif root is None:
            root = el
        else:
            raise XmlError('multiple root elements')
        if not el.self_closing:
            stack.append(el)
    if stack:
        raise XmlError(f'unclosed element <{stack[-1].tag}>')
    if root is None:
        raise XmlError('no root element')
    return Document(prolog, root, epilog)


def load(path: str) -> Document:
    with open(path, 'rb') as f:
        return parse(f.read())


def save(doc: Document, path: str) -> None:
    import os, tempfile
    data = doc.to_bytes()
    d = os.path.dirname(os.path.abspath(path))
    fd, tmp = tempfile.mkstemp(dir=d, prefix='.keel-', suffix='.tmp')
    try:
        with os.fdopen(fd, 'wb') as f:
            f.write(data)
        os.replace(tmp, path)          # atomic on the same filesystem
    except BaseException:
        if os.path.exists(tmp): os.unlink(tmp)
        raise
