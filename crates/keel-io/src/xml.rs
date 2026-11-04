//! Lossless XML for Stormworks files.
//!
//! Unmodified elements re-emit their exact source text, so a parse → write round trip is byte-identical.
//! Edited elements (attribute or child-list changes) are re-serialised locally. Names may start with a digit
//! (the game writes `<physics_shape_rotation 00="1" .../>`).

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlError {
    pub offset: usize,
    pub message: String,
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "xml error at byte {}: {}", self.offset, self.message)
    }
}
impl std::error::Error for XmlError {}

fn err<T>(offset: usize, message: impl Into<String>) -> Result<T, XmlError> {
    Err(XmlError { offset, message: message.into() })
}

#[derive(Debug, Clone)]
pub struct Attr {
    pub name: String,
    /// Quoted raw value exactly as in the source, e.g. `"1,0,0"`.
    pub raw: String,
    pub lead_ws: String,
    pub eq: String,
}

impl Attr {
    pub fn value(&self) -> String {
        unescape(&self.raw[1..self.raw.len() - 1])
    }
}

#[derive(Debug, Clone)]
pub enum Node {
    Element(Element),
    /// Raw text, whitespace, comments, processing instructions, CDATA (kept verbatim).
    Raw(String),
}

#[derive(Debug, Clone, Default)]
pub struct Element {
    pub tag: String,
    pub attrs: Vec<Attr>,
    pub children: Vec<Node>,
    pub self_closing: bool,
    pub tail_ws: String,
    src_start: Option<String>,
    src_end: Option<String>,
    dirty: bool,
}

impl Element {
    pub fn new(tag: &str) -> Self {
        Element { tag: tag.to_string(), self_closing: true, dirty: true, ..Default::default() }
    }
    pub fn get(&self, name: &str) -> Option<String> {
        self.attrs.iter().find(|a| a.name == name).map(|a| a.value())
    }
    pub fn set(&mut self, name: &str, value: &str) {
        let raw = format!("\"{}\"", escape_attr(value));
        if let Some(a) = self.attrs.iter_mut().find(|a| a.name == name) {
            if a.raw != raw {
                a.raw = raw;
                self.dirty = true;
            }
        } else {
            self.attrs.push(Attr { name: name.into(), raw, lead_ws: " ".into(), eq: "=".into() });
            self.dirty = true;
        }
    }
    pub fn remove_attr(&mut self, name: &str) {
        let n = self.attrs.len();
        self.attrs.retain(|a| a.name != name);
        if self.attrs.len() != n {
            self.dirty = true;
        }
    }
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|c| if let Node::Element(e) = c { Some(e) } else { None })
    }
    pub fn elements_mut(&mut self) -> impl Iterator<Item = &mut Element> {
        self.children.iter_mut().filter_map(|c| if let Node::Element(e) = c { Some(e) } else { None })
    }
    pub fn find(&self, tag: &str) -> Option<&Element> {
        self.elements().find(|e| e.tag == tag)
    }
    pub fn find_mut(&mut self, tag: &str) -> Option<&mut Element> {
        self.elements_mut().find(|e| e.tag == tag)
    }
    pub fn push(&mut self, child: Element) {
        if self.self_closing {
            self.self_closing = false;
            self.dirty = true;
        }
        if self.src_end.as_deref().unwrap_or("").is_empty() {
            self.dirty = true;
        }
        self.children.push(Node::Element(child));
    }
    /// Insert a child element at child index `idx` (clamped).
    pub fn insert(&mut self, idx: usize, child: Element) {
        if self.self_closing {
            self.self_closing = false;
            self.dirty = true;
        }
        let i = idx.min(self.children.len());
        self.children.insert(i, Node::Element(child));
    }
    /// Keep only the child elements for which `keep` is true (raw text is kept).
    pub fn retain_elements(&mut self, mut keep: impl FnMut(&Element) -> bool) {
        self.children.retain(|c| match c {
            Node::Element(e) => keep(e),
            Node::Raw(_) => true,
        });
    }
    /// Depth-first visit of this element and all descendants.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Element)) {
        f(self);
        for c in self.elements() {
            c.walk(f);
        }
    }

    fn write_start(&self, out: &mut String) {
        if !self.dirty {
            if let Some(s) = &self.src_start {
                out.push_str(s);
                return;
            }
        }
        out.push('<');
        out.push_str(&self.tag);
        for a in &self.attrs {
            out.push_str(if a.lead_ws.is_empty() { " " } else { &a.lead_ws });
            out.push_str(&a.name);
            out.push_str(if a.eq.is_empty() { "=" } else { &a.eq });
            out.push_str(&a.raw);
        }
        out.push_str(&self.tail_ws);
        out.push_str(if self.self_closing { "/>" } else { ">" });
    }

    pub fn write(&self, out: &mut String) {
        self.write_start(out);
        if self.self_closing {
            return;
        }
        for c in &self.children {
            match c {
                Node::Element(e) => e.write(out),
                Node::Raw(s) => out.push_str(s),
            }
        }
        match (&self.src_end, self.dirty) {
            (Some(s), false) if !s.is_empty() => out.push_str(s),
            _ => {
                out.push_str("</");
                out.push_str(&self.tag);
                out.push('>');
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Document {
    pub prolog: Vec<String>,
    pub root: Element,
    pub epilog: Vec<String>,
}

impl Document {
    pub fn to_string_lossless(&self) -> String {
        let mut out = String::new();
        for p in &self.prolog {
            out.push_str(p);
        }
        self.root.write(&mut out);
        for p in &self.epilog {
            out.push_str(p);
        }
        out
    }
}

pub fn escape_attr(v: &str) -> String {
    let mut s = String::with_capacity(v.len());
    for c in v.chars() {
        match c {
            '&' => s.push_str("&amp;"),
            '<' => s.push_str("&lt;"),
            '>' => s.push_str("&gt;"),
            '"' => s.push_str("&quot;"),
            _ => s.push(c),
        }
    }
    s
}

pub fn unescape(v: &str) -> String {
    if !v.contains('&') {
        return v.to_string();
    }
    v.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

fn is_name_char(b: u8) -> bool {
    !(b.is_ascii_whitespace() || matches!(b, b'=' | b'/' | b'<' | b'>' | b'"' | b'\''))
}

fn skip_ws(s: &[u8], mut i: usize) -> usize {
    while i < s.len() && s[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

fn find(src: &str, pat: &str, from: usize) -> Option<usize> {
    src[from..].find(pat).map(|p| p + from)
}

pub fn parse(src: &str) -> Result<Document, XmlError> {
    let b = src.as_bytes();
    let n = b.len();
    let mut i = 0usize;
    let mut prolog = Vec::new();
    let mut epilog = Vec::new();
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;

    macro_rules! emit_raw {
        ($s:expr) => {{
            let s: &str = $s;
            if !s.is_empty() {
                if let Some(top) = stack.last_mut() {
                    top.children.push(Node::Raw(s.to_string()));
                } else if root.is_none() {
                    prolog.push(s.to_string());
                } else {
                    epilog.push(s.to_string());
                }
            }
        }};
    }

    while i < n {
        let lt = match find(src, "<", i) {
            Some(p) => p,
            None => {
                emit_raw!(&src[i..]);
                break;
            }
        };
        if lt > i {
            emit_raw!(&src[i..lt]);
        }
        let rest = &src[lt..];
        let special = [("<!--", "-->"), ("<?", "?>"), ("<![CDATA[", "]]>")];
        if let Some((_, close)) = special.iter().find(|(open, _)| rest.starts_with(open)) {
            let e = find(src, close, lt).ok_or(XmlError { offset: lt, message: "unterminated construct".into() })?;
            emit_raw!(&src[lt..e + close.len()]);
            i = e + close.len();
            continue;
        }
        if rest.starts_with("<!") {
            let e = find(src, ">", lt).ok_or(XmlError { offset: lt, message: "unterminated declaration".into() })?;
            emit_raw!(&src[lt..=e]);
            i = e + 1;
            continue;
        }
        let closing = b.get(lt + 1) == Some(&b'/');
        let name_start = lt + 1 + closing as usize;
        let mut j = name_start;
        while j < n && is_name_char(b[j]) {
            j += 1;
        }
        if j == name_start {
            return err(lt, "bad tag");
        }
        let name = &src[name_start..j];
        if closing {
            let e = find(src, ">", j).ok_or(XmlError { offset: lt, message: "unterminated end tag".into() })?;
            let mut el = match stack.pop() {
                Some(el) if el.tag == name => el,
                _ => return err(lt, format!("mismatched </{name}>")),
            };
            el.src_end = Some(src[lt..=e].to_string());
            i = e + 1;
            attach(&mut stack, &mut root, el, lt)?;
            continue;
        }
        // attributes
        let mut attrs = Vec::new();
        loop {
            let ws_end = skip_ws(b, j);
            let a0 = ws_end;
            let mut k = a0;
            while k < n && is_name_char(b[k]) {
                k += 1;
            }
            if k == a0 {
                break;
            }
            let eq0 = k;
            let mut k2 = skip_ws(b, k);
            if b.get(k2) != Some(&b'=') {
                break;
            }
            k2 = skip_ws(b, k2 + 1);
            let q = match b.get(k2) {
                Some(&c @ (b'"' | b'\'')) => c,
                _ => return err(k2, "expected quoted attribute value"),
            };
            let close = b[k2 + 1..].iter().position(|&c| c == q).ok_or(XmlError { offset: k2, message: "unterminated attribute".into() })? + k2 + 1;
            attrs.push(Attr {
                lead_ws: src[j..a0].to_string(),
                name: src[a0..eq0].to_string(),
                eq: src[eq0..k2].to_string(),
                raw: src[k2..=close].to_string(),
            });
            j = close + 1;
        }
        let ws_end = skip_ws(b, j);
        let mut el = Element { tag: name.to_string(), attrs, tail_ws: src[j..ws_end].to_string(), ..Default::default() };
        if src[ws_end..].starts_with("/>") {
            el.self_closing = true;
            el.src_start = Some(src[lt..ws_end + 2].to_string());
            el.src_end = Some(String::new());
            i = ws_end + 2;
            attach(&mut stack, &mut root, el, lt)?;
        } else if src[ws_end..].starts_with('>') {
            el.src_start = Some(src[lt..=ws_end].to_string());
            i = ws_end + 1;
            stack.push(el);
        } else {
            return err(lt, format!("malformed start tag <{name}"));
        }
    }
    if let Some(open) = stack.last() {
        return err(n, format!("unclosed element <{}>", open.tag));
    }
    let root = root.ok_or(XmlError { offset: 0, message: "no root element".into() })?;
    Ok(Document { prolog, root, epilog })
}

fn attach(stack: &mut [Element], root: &mut Option<Element>, el: Element, at: usize) -> Result<(), XmlError> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(Node::Element(el));
        Ok(())
    } else if root.is_none() {
        *root = Some(el);
        Ok(())
    } else {
        err(at, "multiple root elements")
    }
}
