//! Expression evaluator for the function components (RE-0013 §5): `+ - * / % ^`, comparisons, `& | !` (and the
//! words `and or not`), constants `pi` and `pi2`, and the functions clamp, len, abs, round, floor, ceil, lerp, atan2,
//! atan, asin, acos, sin, cos, tan, sqrt, min, max, sgn, exp, log. In boolean expressions `^` is exclusive or.
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Id(String),
    Op(&'static str),
    LP,
    RP,
    Comma,
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = vec![];
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() || (ch == '.' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let st = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.') {
                i += 1;
            }
            if i < c.len() && (c[i] == 'e' || c[i] == 'E') && c.get(i + 1).is_some_and(|d| d.is_ascii_digit() || *d == '-' || *d == '+') {
                i += 2;
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let t: String = c[st..i].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| format!("bad number {t}"))?));
        } else if ch.is_alphabetic() || ch == '_' {
            let st = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            let w: String = c[st..i].iter().collect::<String>().to_lowercase();
            out.push(match w.as_str() {
                "and" => Tok::Op("&"),
                "or" => Tok::Op("|"),
                "not" => Tok::Op("!"),
                _ => Tok::Id(w),
            });
        } else {
            let two: String = c[i..(i + 2).min(c.len())].iter().collect();
            let op2 = ["<=", ">=", "==", "!=", "~=", "&&", "||", "!&", "!|", "!^"].into_iter().find(|o| *o == two);
            if let Some(o) = op2 {
                out.push(Tok::Op(match o {
                    "~=" => "!=",
                    "&&" => "&",
                    "||" => "|",
                    o => o,
                }));
                i += 2;
                continue;
            }
            out.push(match ch {
                '(' => Tok::LP,
                ')' => Tok::RP,
                ',' => Tok::Comma,
                '+' => Tok::Op("+"),
                '-' => Tok::Op("-"),
                '*' => Tok::Op("*"),
                '/' => Tok::Op("/"),
                '%' => Tok::Op("%"),
                '^' => Tok::Op("^"),
                '<' => Tok::Op("<"),
                '>' => Tok::Op(">"),
                '=' => Tok::Op("=="),
                '!' | '~' => Tok::Op("!"),
                '&' => Tok::Op("&"),
                '|' => Tok::Op("|"),
                _ => return Err(format!("unexpected '{ch}'")),
            });
            i += 1;
        }
    }
    Ok(out)
}

struct P<'a> {
    t: Vec<Tok>,
    i: usize,
    vars: &'a HashMap<&'a str, f64>,
    boolean: bool,
}

fn b(x: bool) -> f64 {
    if x {
        1.0
    } else {
        0.0
    }
}

impl P<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn eat_op(&mut self, ops: &[&str]) -> Option<&'static str> {
        if let Some(Tok::Op(o)) = self.peek() {
            if ops.contains(o) {
                let o = *o;
                self.i += 1;
                return Some(o);
            }
        }
        None
    }
    fn or(&mut self) -> Result<f64, String> {
        let mut a = self.and()?;
        while let Some(o) = self.eat_op(&["|", "!|"]) {
            let c = self.and()?;
            a = b((a != 0.0 || c != 0.0) != (o == "!|"));
        }
        Ok(a)
    }
    fn and(&mut self) -> Result<f64, String> {
        let mut a = self.cmp()?;
        while let Some(o) = self.eat_op(&["&", "!&", "!^"]) {
            let c = self.cmp()?;
            a = match o {
                "&" => b(a != 0.0 && c != 0.0),
                "!&" => b(!(a != 0.0 && c != 0.0)),
                _ => b((a != 0.0) == (c != 0.0)),
            };
        }
        Ok(a)
    }
    fn cmp(&mut self) -> Result<f64, String> {
        let a = self.add()?;
        if let Some(o) = self.eat_op(&["<", ">", "<=", ">=", "==", "!="]) {
            let c = self.add()?;
            return Ok(b(match o {
                "<" => a < c,
                ">" => a > c,
                "<=" => a <= c,
                ">=" => a >= c,
                "==" => a == c,
                _ => a != c,
            }));
        }
        Ok(a)
    }
    fn add(&mut self) -> Result<f64, String> {
        let mut a = self.mul()?;
        while let Some(o) = self.eat_op(&["+", "-"]) {
            let c = self.mul()?;
            a = if o == "+" { a + c } else { a - c };
        }
        Ok(a)
    }
    fn mul(&mut self) -> Result<f64, String> {
        let mut a = self.unary()?;
        while let Some(o) = self.eat_op(&["*", "/", "%"]) {
            let c = self.unary()?;
            a = match o {
                "*" => a * c,
                "/" => a / c,
                _ => a % c,
            };
        }
        Ok(a)
    }
    fn unary(&mut self) -> Result<f64, String> {
        if self.eat_op(&["-"]).is_some() {
            return Ok(-self.unary()?);
        }
        if self.eat_op(&["!"]).is_some() {
            return Ok(b(self.unary()? == 0.0));
        }
        self.pow()
    }
    fn pow(&mut self) -> Result<f64, String> {
        let a = self.primary()?;
        if self.eat_op(&["^"]).is_some() {
            let c = self.unary()?;
            return Ok(if self.boolean { b((a != 0.0) != (c != 0.0)) } else { a.powf(c) });
        }
        Ok(a)
    }
    fn primary(&mut self) -> Result<f64, String> {
        match self.t.get(self.i).cloned() {
            Some(Tok::Num(v)) => {
                self.i += 1;
                Ok(v)
            }
            Some(Tok::LP) => {
                self.i += 1;
                let v = self.or()?;
                if self.peek() != Some(&Tok::RP) {
                    return Err("missing ')'".into());
                }
                self.i += 1;
                Ok(v)
            }
            Some(Tok::Id(name)) => {
                self.i += 1;
                if self.peek() == Some(&Tok::LP) {
                    self.i += 1;
                    let mut args = vec![];
                    if self.peek() != Some(&Tok::RP) {
                        loop {
                            args.push(self.or()?);
                            if self.peek() == Some(&Tok::Comma) {
                                self.i += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    if self.peek() != Some(&Tok::RP) {
                        return Err(format!("missing ')' after {name}("));
                    }
                    self.i += 1;
                    return call(&name, &args);
                }
                match name.as_str() {
                    "pi" => Ok(std::f64::consts::PI),
                    "pi2" => Ok(std::f64::consts::TAU),
                    "true" => Ok(1.0),
                    "false" => Ok(0.0),
                    v => self.vars.get(v).copied().ok_or_else(|| format!("unknown name '{v}'")),
                }
            }
            Some(t) => Err(format!("unexpected {t:?}")),
            None => Err("unexpected end".into()),
        }
    }
}

fn call(f: &str, a: &[f64]) -> Result<f64, String> {
    let need = |n: usize| if a.len() == n { Ok(()) } else { Err(format!("{f} takes {n} argument(s)")) };
    Ok(match f {
        "abs" => (need(1)?, a[0].abs()).1,
        // round(x, step) appears in vehicles; KEEL rounds to a multiple of step (unconfirmed, OPEN-ITEMS A8)
        "round" if a.len() == 2 && a[1] != 0.0 => (a[0] / a[1]).round() * a[1],
        "round" => (need(1)?, a[0].round()).1,
        "floor" => (need(1)?, a[0].floor()).1,
        "ceil" => (need(1)?, a[0].ceil()).1,
        "sin" => (need(1)?, a[0].sin()).1,
        "cos" => (need(1)?, a[0].cos()).1,
        "tan" => (need(1)?, a[0].tan()).1,
        "asin" => (need(1)?, a[0].asin()).1,
        "acos" => (need(1)?, a[0].acos()).1,
        "atan" => (need(1)?, a[0].atan()).1,
        "sqrt" => (need(1)?, a[0].sqrt()).1,
        "exp" => (need(1)?, a[0].exp()).1,
        "log" => (need(1)?, a[0].ln()).1,
        "sgn" => (need(1)?, if a[0] > 0.0 { 1.0 } else if a[0] < 0.0 { -1.0 } else { 0.0 }).1,
        "atan2" => (need(2)?, a[0].atan2(a[1])).1,
        "min" if !a.is_empty() => a.iter().copied().fold(f64::INFINITY, f64::min),
        "max" if !a.is_empty() => a.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        "len" => a.iter().map(|x| x * x).sum::<f64>().sqrt(),
        "clamp" => (need(3)?, a[0].clamp(a[1].min(a[2]), a[1].max(a[2]))).1,
        "lerp" => (need(3)?, a[0] + (a[1] - a[0]) * a[2]).1,
        _ => return Err(format!("unknown function {f}")),
    })
}

/// Evaluate `src` with the given variables. `boolean` makes `^` exclusive-or.
pub fn eval(src: &str, vars: &HashMap<&str, f64>, boolean: bool) -> Result<f64, String> {
    let t = lex(src)?;
    if t.is_empty() {
        return Ok(0.0);
    }
    let mut p = P { t, i: 0, vars, boolean };
    let v = p.or()?;
    if p.i != p.t.len() {
        return Err(format!("unexpected {:?}", p.t[p.i]));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn evaluates() {
        let v: HashMap<&str, f64> = [("x", 2.0), ("y", 3.0)].into_iter().collect();
        assert_eq!(eval("x*y+1", &v, false), Ok(7.0));
        assert_eq!(eval("x^y", &v, false), Ok(8.0));
        assert_eq!(eval("-x^2", &v, false), Ok(-4.0));
        assert_eq!(eval("clamp(x*10, 0, 5)", &v, false), Ok(5.0));
        assert_eq!(eval("(-x)/2", &v, false), Ok(-1.0));
        assert_eq!(eval("x & !y | x", &v, true), Ok(1.0));
        assert_eq!(eval("x ^ y", &v, true), Ok(0.0));
        assert!(eval("q+1", &v, false).is_err());
        assert_eq!(eval("((x!^y)&x)", &v, true), Ok(1.0));
    }
}
