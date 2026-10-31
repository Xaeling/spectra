//! ZIP 容器读写：基于 `zip` crate，store（不压缩）模式。
//!
//! # 为什么换成 `zip` crate
//! 之前是手写 Local Header / Central Directory / EOCD 三段。store 模式的
//! 格式本身不长，但 ZIP64、数据描述符、EOCD 注释长度这些边界自己写容易漏，
//! 读第三方产生的文件时尤其容易解析失败。交给成熟库更可靠，也省得维护。
//!
//! 仍然只用 **store** 模式：`.sprx` / `.prism` 的内容主要是 CSV 与 JSON，
//! 压缩收益有限，而不压缩能让 `mimetype` 之类的条目保持可读、便于排查。
//!
//! # 对外接口保持不变
//! `Entry` / `ReadEntry` / `write` / `read` / `get_text` / `has` 的签名
//! 与语义都没变，`sprx.rs` 不需要跟着改。

use std::io::{Read, Write};

/// 一个待写入的条目。
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

/// 一个读出来的条目。
///
/// 与 `Entry` 字段相同但语义不同：`Entry` 是输出前的待写项，
/// `ReadEntry` 是从容器里解出来的结果。分开命名是为了让 `read` /
/// `get_text` / `has` 的签名保持与替换成 `zip` crate 之前一致。
#[derive(Clone, Debug)]
pub struct ReadEntry {
    pub name: String,
    pub data: Vec<u8>,
}

impl Entry {
    pub fn new(name: impl Into<String>, data: Vec<u8>) -> Self {
        Self { name: name.into(), data }
    }
    /// 文本条目（按 UTF-8 存）。
    pub fn text(name: impl Into<String>, s: &str) -> Self {
        Self::new(name, s.as_bytes().to_vec())
    }
}

// ══════════════════════════════════════════════════════════════════════
// 写端
// ══════════════════════════════════════════════════════════════════════

/// 把若干条目打包成一个 ZIP 字节流（store 模式，不压缩）。
///
/// 用 `zip` crate 而不是手写容器：store 模式的 Local Header /
/// Central Directory / EOCD 三段虽然不长，但目录结束记录、ZIP64、
/// 数据描述符这些边界情况自己写容易漏，交给成熟库更可靠。
///
/// 条目顺序**原样保留** —— `.sprx` 的 `mimetype` 必须是第一项，
/// 读端才能只读开头就判定格式。
pub fn write(entries: &[Entry]) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut buf));
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for e in entries {
            // 单条目失败不应毁掉整个文件：跳过它并继续。
            if w.start_file(e.name.clone(), opts).is_err() {
                continue;
            }
            if w.write_all(&e.data).is_err() {
                continue;
            }
        }
        let _ = w.finish();
    }
    buf
}

/// 解析 ZIP，返回全部条目。
///
/// 委托给 `zip` crate：它负责 EOCD 定位、CRC 校验与（若遇到）deflate 解压。
/// 失败的条目直接跳过，不让一个坏项毁掉整份工程文件。
pub fn read(bytes: &[u8]) -> Result<Vec<ReadEntry>, String> {
    let mut out: Vec<ReadEntry> = Vec::new();
    if !looks_like_zip(bytes) {
        return Err("不是 ZIP 容器（未找到 PK 签名）".to_string());
    }
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("无法解析 ZIP：{}", e))?;
    for i in 0..z.len() {
        let mut f = match z.by_index(i) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let name = f.name().to_string();
        let mut data = Vec::with_capacity(f.size() as usize);
        match f.read_to_end(&mut data) {
            Ok(_) => {}
            Err(_) => {
                // CRC 校验失败会在这里暴露；跳过该条目而不是整体失败
                continue;
            }
        }
        out.push(ReadEntry { name, data });
    }
    if out.is_empty() {
        return Err("ZIP 内没有任何可读条目".to_string());
    }
    Ok(out)
}

/// 快速判定：开头是不是本地文件头签名 `PK\x03\x04`。
fn looks_like_zip(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == 0x50 && bytes[1] == 0x4B && bytes[2] == 0x03 && bytes[3] == 0x04
}

/// 按名字取出条目的文本（UTF-8）。
pub fn get_text<'a>(entries: &'a [ReadEntry], name: &str) -> Option<&'a str> {
    entries
        .iter()
        .find(|e| e.name == name)
        .and_then(|e| std::str::from_utf8(&e.data).ok())
}

/// 是否存在该名字的条目。
pub fn has(entries: &[ReadEntry], name: &str) -> bool {
    entries.iter().any(|e| e.name == name)
}
