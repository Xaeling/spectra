//! 计算列用的表达式求值器（纯 Rust、无依赖）。
//!
//! 语法（大小写敏感）：
//! ```text
//! 数字      3   2.5   .5
//! 列引用    {列名}        —— 名字原样匹配（支持中文）
//! 常量      pi  e
//! 运算      + - * / % ^   （^ 右结合；一元负号优先级低于 ^，-2^2 = -4）
//! 括号      ( … )
//! 函数      ln log10 log2 sqrt abs exp sin cos tan round floor ceil   （1 参）
//!           min max pow atan2                                        （2 参）
//! ```
//!
//! `eval` 直接求值（不建 AST）：公式短、行数级求值开销可忽略。

/// 求一个表达式。`resolve` 把列名解析成当前行的数值（缺失返回 None）。
pub fn eval(formula: &str, resolve: &dyn Fn(&str) -> Option<f64>) -> Result<f64, String> {
    let mut p = Parser {
        chars: formula.chars().collect(),
        i: 0,
        resolve,
    };
    let v = p.expr()?;
    p.ws();
    if p.i < p.chars.len() {
        return Err(format!(
            "公式在第 {} 个字符处有多余内容：{:?}",
            p.i + 1,
            p.rest_snippet()
        ));
    }
    if !v.is_finite() {
        return Err("结果不是有限数（除零或溢出）".into());
    }
    Ok(v)
}

struct Parser<'a> {
    chars: Vec<char>,
    i: usize,
    resolve: &'a dyn Fn(&str) -> Option<f64>,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.chars.len() && self.chars[self.i].is_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.ws();
        self.chars.get(self.i).copied()
    }

    fn rest_snippet(&self) -> String {
        self.chars.iter().skip(self.i).take(12).collect()
    }

    fn eat(&mut self, c: char) -> bool {
        self.ws();
        if self.chars.get(self.i) == Some(&c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expr(&mut self) -> Result<f64, String> {
        let mut v = self.term()?;
        loop {
            match self.peek() {
                Some('+') => {
                    self.i += 1;
                    v += self.term()?;
                }
                Some('-') => {
                    self.i += 1;
                    v -= self.term()?;
                }
                _ => return Ok(v),
            }
        }
    }

    fn term(&mut self) -> Result<f64, String> {
        let mut v = self.unary()?;
        loop {
            match self.peek() {
                Some('*') => {
                    self.i += 1;
                    v *= self.unary()?;
                }
                Some('/') => {
                    self.i += 1;
                    let d = self.unary()?;
                    if d == 0.0 {
                        return Err("除数为 0".into());
                    }
                    v /= d;
                }
                Some('%') => {
                    self.i += 1;
                    let d = self.unary()?;
                    if d == 0.0 {
                        return Err("取余除数为 0".into());
                    }
                    v %= d;
                }
                _ => return Ok(v),
            }
        }
    }

    fn unary(&mut self) -> Result<f64, String> {
        match self.peek() {
            Some('-') => {
                self.i += 1;
                Ok(-self.unary()?)
            }
            Some('+') => {
                self.i += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<f64, String> {
        let base = self.primary()?;
        if self.eat('^') {
            // 右结合：2^3^2 = 2^(3^2)
            let exp = self.unary()?;
            Ok(base.powf(exp))
        } else {
            Ok(base)
        }
    }

    fn primary(&mut self) -> Result<f64, String> {
        self.ws();
        let c = self
            .chars
            .get(self.i)
            .copied()
            .ok_or_else(|| "公式意外结束".to_string())?;
        if c == '(' {
            self.i += 1;
            let v = self.expr()?;
            if !self.eat(')') {
                return Err("缺少右括号".into());
            }
            return Ok(v);
        }
        if c == '{' {
            self.i += 1;
            let start = self.i;
            while self.i < self.chars.len() && self.chars[self.i] != '}' {
                self.i += 1;
            }
            if self.i >= self.chars.len() {
                return Err("列名缺少右花括号 }".into());
            }
            let name: String = self.chars[start..self.i].iter().collect();
            self.i += 1; // consume }
            let name = name.trim().to_string();
            return (self.resolve)(&name).ok_or_else(|| format!("列「{name}」在该行没有数值"));
        }
        if c.is_ascii_digit() || c == '.' {
            let start = self.i;
            while self.i < self.chars.len()
                && (self.chars[self.i].is_ascii_digit() || self.chars[self.i] == '.')
            {
                self.i += 1;
            }
            // 科学计数法 1e-3
            if self.i < self.chars.len() && (self.chars[self.i] == 'e' || self.chars[self.i] == 'E')
            {
                let save = self.i;
                self.i += 1;
                if self.i < self.chars.len()
                    && (self.chars[self.i] == '+' || self.chars[self.i] == '-')
                {
                    self.i += 1;
                }
                let d0 = self.i;
                while self.i < self.chars.len() && self.chars[self.i].is_ascii_digit() {
                    self.i += 1;
                }
                if d0 == self.i {
                    self.i = save; // 不是指数，回退
                }
            }
            let s: String = self.chars[start..self.i].iter().collect();
            return s.parse::<f64>().map_err(|_| format!("无法解析数字「{s}」"));
        }
        if c.is_alphabetic() || c == '_' {
            let start = self.i;
            while self.i < self.chars.len()
                && (self.chars[self.i].is_alphanumeric() || self.chars[self.i] == '_')
            {
                self.i += 1;
            }
            let ident: String = self.chars[start..self.i].iter().collect();
            let lower = ident.to_ascii_lowercase();
            match lower.as_str() {
                "pi" => return Ok(std::f64::consts::PI),
                "e" => return Ok(std::f64::consts::E),
                _ => {}
            }
            if !self.eat('(') {
                return Err(format!(
                    "未知名称「{ident}」（列用 {{列名}} 引用，函数要带括号）"
                ));
            }
            let mut args = Vec::new();
            if !self.eat(')') {
                loop {
                    args.push(self.expr()?);
                    if self.eat(')') {
                        break;
                    }
                    if !self.eat(',') {
                        return Err(format!("函数 {ident} 的参数之间应为逗号"));
                    }
                }
            }
            return call_fn(&lower, &args).ok_or_else(|| {
                format!("未知函数「{ident}」或参数个数不对（支持 ln/log10/log2/sqrt/abs/exp/sin/cos/tan/round/floor/ceil、min/max/pow/atan2）")
            });
        }
        Err(format!("无法识别的字符「{c}」"))
    }
}

fn call_fn(name: &str, a: &[f64]) -> Option<f64> {
    let one = |f: fn(f64) -> f64| -> Option<f64> {
        if a.len() == 1 {
            Some(f(a[0]))
        } else {
            None
        }
    };
    match name {
        "ln" => one(f64::ln),
        "log10" => one(f64::log10),
        "log2" => one(f64::log2),
        "sqrt" => one(f64::sqrt),
        "abs" => one(f64::abs),
        "exp" => one(f64::exp),
        "sin" => one(f64::sin),
        "cos" => one(f64::cos),
        "tan" => one(f64::tan),
        "round" => one(f64::round),
        "floor" => one(f64::floor),
        "ceil" => one(f64::ceil),
        "min" | "max" | "pow" | "atan2" => {
            if a.len() == 2 {
                Some(match name {
                    "min" => a[0].min(a[1]),
                    "max" => a[0].max(a[1]),
                    "pow" => a[0].powf(a[1]),
                    _ => a[0].atan2(a[1]),
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_cols(_: &str) -> Option<f64> {
        None
    }

    #[test]
    fn 运算优先级与括号() {
        assert!((eval("1+2*3", &no_cols).unwrap() - 7.0).abs() < 1e-12);
        assert!((eval("(1+2)*3", &no_cols).unwrap() - 9.0).abs() < 1e-12);
        assert!((eval("10/4", &no_cols).unwrap() - 2.5).abs() < 1e-12);
        assert!((eval("7%3", &no_cols).unwrap() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn 幂右结合与负号优先级() {
        // 2^3^2 = 2^(3^2) = 512；-2^2 = -(2^2) = -4（数学惯例）
        assert!((eval("2^3^2", &no_cols).unwrap() - 512.0).abs() < 1e-9);
        assert!((eval("-2^2", &no_cols).unwrap() + 4.0).abs() < 1e-12);
        assert!((eval("(-2)^2", &no_cols).unwrap() - 4.0).abs() < 1e-12);
    }

    #[test]
    fn 常量与函数() {
        assert!((eval("ln(e)", &no_cols).unwrap() - 1.0).abs() < 1e-12);
        assert!((eval("sqrt(16)", &no_cols).unwrap() - 4.0).abs() < 1e-12);
        assert!((eval("max(3, 5) * 2", &no_cols).unwrap() - 10.0).abs() < 1e-12);
        assert!((eval("abs(0-3)", &no_cols).unwrap() - 3.0).abs() < 1e-12);
    }

    #[test]
    fn 列引用按名字解析() {
        let f = |name: &str| match name {
            "体重" => Some(60.0),
            "身高" => Some(1.7),
            _ => None,
        };
        assert!((eval("{体重} / ({身高}^2)", &f).unwrap() - 60.0 / 2.89).abs() < 1e-9);
        assert!(eval("{不存在} * 2", &f).is_err());
    }

    #[test]
    fn 错误路径有说明() {
        let e = eval("1 + ", &no_cols).unwrap_err();
        assert!(e.contains("意外结束"), "{e}");
        let e = eval("sin(1, 2)", &no_cols).unwrap_err();
        assert!(e.contains("参数个数"), "{e}");
        assert!(eval("1/0", &no_cols).is_err());
        assert!(eval("foo(1)", &no_cols).is_err());
    }

    #[test]
    fn 计算列随源数据重算() {
        use crate::core::model::{Cell, ColRole, Column, DataTable, TableKind};
        let mut t = DataTable::template(TableKind::Column, String::from("t"), 2);
        t.columns.clear();
        let mut a = Column::new("a", ColRole::Y);
        a.cells = vec![Cell::number(1.0), Cell::number(2.0)];
        let mut b = Column::new("b", ColRole::Y);
        b.cells = vec![Cell::number(10.0), Cell::number(20.0)];
        t.columns.push(a);
        t.columns.push(b);
        let idx = t.add_computed_column("c", "{a}*2+{b}").expect("add");
        let vals: Vec<f64> = t.columns[idx].cells.iter().filter_map(|c| c.num).collect();
        assert_eq!(vals, vec![12.0, 24.0]);
        // 改源数据 → recompute → 联动更新
        t.columns[0].cells[0] = Cell::number(5.0);
        t.recompute();
        assert!((t.columns[idx].cells[0].num.unwrap() - 20.0).abs() < 1e-12);
        // 错误公式 / 重名列被拒绝
        assert!(t.add_computed_column("bad", "{nope}+1").is_err());
        assert!(t.add_computed_column("a", "1").is_err());
    }
}
