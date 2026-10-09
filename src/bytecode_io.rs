//! ZeroScript bytecode file I/O (§158, Bytecode.md).
//!
//! File layout: `payload || sha256(payload)` — 32 raw bytes appended after
//! the payload (§158, ISSUES §73a). The checksum is verified *before* any
//! parsing; the magic/version inside the payload stay subject to the VM's
//! §124/§125 validation at run time, keeping integrity and compatibility as
//! separate concerns.
//!
//! Byte order is little-endian. Field order and opcode tag values are the
//! mechanical decisions recorded in ISSUES §73a: tags are explicit literals
//! mirroring the declaration order of `Opcode` in compiler.rs; the encode
//! `match` is exhaustive so a new variant is a compile error until this
//! codec is updated. A tag this build does not recognize decodes to
//! `Opcode::Unknown` so §124 rejects the stream as a VM Error (ISSUES §68e)
//! instead of a format error.

use crate::compiler::{BytecodeHeader, BytecodeProgram, Constant, FunctionInfo, Opcode};
use crate::hash::Sha256;
use crate::module::{ModuleBundle, ModuleMeta};

/// Length of the trailing SHA-256 checksum (§158: 32 byte).
pub const CHECKSUM_LEN: usize = 32;

#[derive(Debug, PartialEq, Eq)]
pub enum BytecodeFileError {
    /// Fewer than CHECKSUM_LEN bytes — the file cannot even carry a hash.
    Truncated,
    /// Trailing 32 bytes are not the SHA-256 of the payload.
    BadChecksum,
    /// Payload parses but is structurally inconsistent (§158 "struktur file
    /// tidak terbaca").
    Malformed(String),
}

impl std::fmt::Display for BytecodeFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BytecodeFileError::Truncated => write!(f, "truncated bytecode file"),
            BytecodeFileError::BadChecksum => write!(f, "bytecode checksum mismatch"),
            BytecodeFileError::Malformed(detail) => {
                write!(f, "malformed bytecode file: {}", detail)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Encoding
// ---------------------------------------------------------------------------

fn put_u8(out: &mut Vec<u8>, v: u8) {
    out.push(v);
}

fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_usize(out: &mut Vec<u8>, v: usize) {
    put_u64(out, v as u64);
}

fn put_f64(out: &mut Vec<u8>, v: f64) {
    put_u64(out, v.to_bits());
}

fn put_bool(out: &mut Vec<u8>, v: bool) {
    put_u8(out, u8::from(v));
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u64(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

fn put_opt_usize(out: &mut Vec<u8>, v: Option<usize>) {
    match v {
        None => put_u8(out, 0),
        Some(x) => {
            put_u8(out, 1);
            put_usize(out, x);
        }
    }
}

fn put_vec<T>(out: &mut Vec<u8>, items: &[T], f: impl Fn(&mut Vec<u8>, &T)) {
    put_u64(out, items.len() as u64);
    for item in items {
        f(out, item);
    }
}

fn put_opcode(out: &mut Vec<u8>, op: &Opcode) {
    // Tags = explicit literals mirroring Opcode's declaration order (ISSUES §73a).
    match op {
        Opcode::LoadConst(v) => {
            put_u32(out, 0);
            put_usize(out, *v);
        }
        Opcode::LoadFunction(v) => {
            put_u32(out, 1);
            put_usize(out, *v);
        }
        Opcode::DeclareLocal(name, is_const) => {
            put_u32(out, 2);
            put_str(out, name);
            put_bool(out, *is_const);
        }
        Opcode::LoadLocal(name) => {
            put_u32(out, 3);
            put_str(out, name);
        }
        Opcode::StoreLocal(name) => {
            put_u32(out, 4);
            put_str(out, name);
        }
        Opcode::DeclareGlobal(name, is_const) => {
            put_u32(out, 5);
            put_str(out, name);
            put_bool(out, *is_const);
        }
        Opcode::LoadGlobal(name) => {
            put_u32(out, 6);
            put_str(out, name);
        }
        Opcode::StoreGlobal(name) => {
            put_u32(out, 7);
            put_str(out, name);
        }
        Opcode::PushScope => put_u32(out, 8),
        Opcode::PopScope => put_u32(out, 9),
        Opcode::Add => put_u32(out, 10),
        Opcode::Sub => put_u32(out, 11),
        Opcode::Mul => put_u32(out, 12),
        Opcode::Div => put_u32(out, 13),
        Opcode::Mod => put_u32(out, 14),
        Opcode::Pow => put_u32(out, 15),
        Opcode::Neg => put_u32(out, 16),
        Opcode::Pos => put_u32(out, 17),
        Opcode::Not => put_u32(out, 18),
        Opcode::Eq => put_u32(out, 19),
        Opcode::Neq => put_u32(out, 20),
        Opcode::RawEq => put_u32(out, 21),
        Opcode::Gt => put_u32(out, 22),
        Opcode::Lt => put_u32(out, 23),
        Opcode::Gte => put_u32(out, 24),
        Opcode::Lte => put_u32(out, 25),
        Opcode::And => put_u32(out, 26),
        Opcode::Or => put_u32(out, 27),
        Opcode::Pop => put_u32(out, 28),
        Opcode::Dup => put_u32(out, 29),
        Opcode::Swap => put_u32(out, 30),
        Opcode::Jump(target) => {
            put_u32(out, 31);
            put_usize(out, *target);
        }
        Opcode::JumpIfTrue(target) => {
            put_u32(out, 32);
            put_usize(out, *target);
        }
        Opcode::JumpIfFalse(target) => {
            put_u32(out, 33);
            put_usize(out, *target);
        }
        Opcode::TryPush { catch, finally } => {
            put_u32(out, 34);
            put_opt_usize(out, *catch);
            put_opt_usize(out, *finally);
        }
        Opcode::TryPop => put_u32(out, 35),
        Opcode::FinallyEnd => put_u32(out, 36),
        Opcode::Call(argc) => {
            put_u32(out, 37);
            put_usize(out, *argc);
        }
        Opcode::CallNative(name, argc) => {
            put_u32(out, 38);
            put_str(out, name);
            put_usize(out, *argc);
        }
        Opcode::Return => put_u32(out, 39),
        Opcode::MakeClosure(function_index, captured) => {
            put_u32(out, 40);
            put_usize(out, *function_index);
            put_vec(out, captured, |out, (name, is_const)| {
                put_str(out, name);
                put_bool(out, *is_const);
            });
        }
        Opcode::LoadCaptured(name) => {
            put_u32(out, 41);
            put_str(out, name);
        }
        Opcode::StoreCaptured(name) => {
            put_u32(out, 42);
            put_str(out, name);
        }
        Opcode::Await => put_u32(out, 43),
        Opcode::MakeList(len) => {
            put_u32(out, 44);
            put_usize(out, *len);
        }
        Opcode::MakeObject(keys) => {
            put_u32(out, 45);
            put_vec(out, keys, |out, key| put_str(out, key));
        }
        Opcode::ListAdd(argc) => {
            put_u32(out, 46);
            put_usize(out, *argc);
        }
        Opcode::ListRemove(argc) => {
            put_u32(out, 47);
            put_usize(out, *argc);
        }
        Opcode::ObjectRemove(argc) => {
            put_u32(out, 48);
            put_usize(out, *argc);
        }
        Opcode::GetIndex => put_u32(out, 49),
        Opcode::SetIndex => put_u32(out, 50),
        Opcode::GetProperty(name) => {
            put_u32(out, 51);
            put_str(out, name);
        }
        Opcode::SetProperty(name) => {
            put_u32(out, 52);
            put_str(out, name);
        }
        Opcode::Print => put_u32(out, 53),
        Opcode::Halt => put_u32(out, 54),
        Opcode::EndModule => put_u32(out, 55),
        Opcode::Unknown(tag) => {
            put_u32(out, 56);
            put_u64(out, *tag);
        }
    }
}

fn put_constant(out: &mut Vec<u8>, c: &Constant) {
    match c {
        Constant::Number(v) => {
            put_u32(out, 0);
            put_f64(out, *v);
        }
        Constant::Str(s) => {
            put_u32(out, 1);
            put_str(out, s);
        }
        Constant::Boolean(b) => {
            put_u32(out, 2);
            put_bool(out, *b);
        }
        Constant::Null => put_u32(out, 3),
        Constant::Function(idx) => {
            put_u32(out, 4);
            put_usize(out, *idx);
        }
    }
}

fn put_function_info(out: &mut Vec<u8>, f: &FunctionInfo) {
    put_str(out, &f.name);
    put_vec(out, &f.params, |out, p| put_str(out, p));
    put_vec(out, &f.param_defaults, |out, d| put_opt_usize(out, *d));
    put_usize(out, f.entry_point);
    put_usize(out, f.local_count);
    put_vec(out, &f.captured_vars, |out, v| put_str(out, v));
    put_bool(out, f.is_async);
}

fn put_module_meta(out: &mut Vec<u8>, m: &ModuleMeta) {
    put_str(out, &m.path);
    put_usize(out, m.start);
    put_usize(out, m.end);
    put_vec(out, &m.exports, |out, e| put_str(out, e));
    put_vec(out, &m.imports, |out, (local, dep, export)| {
        put_str(out, local);
        put_str(out, dep);
        put_str(out, export);
    });
    put_vec(out, &m.re_exports, |out, (export, dep)| {
        put_str(out, export);
        put_str(out, dep);
    });
}

fn put_bundle(out: &mut Vec<u8>, bundle: &ModuleBundle) {
    let p = &bundle.program;
    put_str(out, &p.header.magic);
    put_u32(out, p.header.version);
    put_usize(out, p.entry_point);
    put_vec(out, &p.constants, put_constant);
    put_vec(out, &p.instructions, put_opcode);
    put_vec(out, &p.functions, put_function_info);
    put_vec(out, &p.function_bodies, |out, body| {
        put_vec(out, body, put_opcode);
    });
    put_vec(out, &p.default_bodies, |out, body| {
        put_vec(out, body, put_opcode);
    });
    put_vec(out, &bundle.modules, put_module_meta);
}

/// Serialize `bundle` into a §158 file image: payload followed by the raw
/// SHA-256 checksum of that payload.
pub fn save(bundle: &ModuleBundle) -> Vec<u8> {
    let mut payload = Vec::new();
    put_bundle(&mut payload, bundle);
    let mut hasher = Sha256::new();
    hasher.update(&payload);
    payload.extend_from_slice(&hasher.finish());
    payload
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], BytecodeFileError> {
        if self.remaining() < n {
            return Err(BytecodeFileError::Malformed(
                "unexpected end of payload".to_string(),
            ));
        }
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, BytecodeFileError> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> Result<u32, BytecodeFileError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn u64(&mut self) -> Result<u64, BytecodeFileError> {
        let b = self.take(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_le_bytes(a))
    }

    fn usize_(&mut self) -> Result<usize, BytecodeFileError> {
        usize::try_from(self.u64()?).map_err(|_| {
            BytecodeFileError::Malformed("integer does not fit this platform".to_string())
        })
    }

    fn f64(&mut self) -> Result<f64, BytecodeFileError> {
        Ok(f64::from_bits(self.u64()?))
    }

    fn bool(&mut self) -> Result<bool, BytecodeFileError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(BytecodeFileError::Malformed(format!(
                "invalid boolean byte {}",
                other
            ))),
        }
    }

    fn str(&mut self) -> Result<String, BytecodeFileError> {
        let len = self.usize_()?;
        let bytes = self.take(len)?;
        let s = std::str::from_utf8(bytes).map_err(|_| {
            BytecodeFileError::Malformed("string is not valid UTF-8".to_string())
        })?;
        Ok(s.to_string())
    }

    fn opt_usize(&mut self) -> Result<Option<usize>, BytecodeFileError> {
        match self.u8()? {
            0 => Ok(None),
            1 => Ok(Some(self.usize_()?)),
            other => Err(BytecodeFileError::Malformed(format!(
                "invalid option tag {}",
                other
            ))),
        }
    }

    /// Element counts are guarded against the bytes actually present before
    /// any allocation, so a hostile count cannot trigger a huge Vec.
    fn vec<T>(
        &mut self,
        mut f: impl FnMut(&mut Reader<'a>) -> Result<T, BytecodeFileError>,
    ) -> Result<Vec<T>, BytecodeFileError> {
        let count = self.u64()?;
        if count > self.remaining() as u64 {
            return Err(BytecodeFileError::Malformed(format!(
                "vector length {} exceeds payload",
                count
            )));
        }
        let n = usize::try_from(count).map_err(|_| {
            BytecodeFileError::Malformed("vector length does not fit this platform".to_string())
        })?;
        let mut items = Vec::with_capacity(n);
        for _ in 0..n {
            items.push(f(self)?);
        }
        Ok(items)
    }
}

fn get_opcode(r: &mut Reader) -> Result<Opcode, BytecodeFileError> {
    // Mirror of put_opcode's tags; an unknown tag becomes Opcode::Unknown so
    // §124 validation rejects it as a VM Error (ISSUES §68e).
    let op = match r.u32()? {
        0 => Opcode::LoadConst(r.usize_()?),
        1 => Opcode::LoadFunction(r.usize_()?),
        2 => {
            let name = r.str()?;
            let is_const = r.bool()?;
            Opcode::DeclareLocal(name, is_const)
        }
        3 => Opcode::LoadLocal(r.str()?),
        4 => Opcode::StoreLocal(r.str()?),
        5 => {
            let name = r.str()?;
            let is_const = r.bool()?;
            Opcode::DeclareGlobal(name, is_const)
        }
        6 => Opcode::LoadGlobal(r.str()?),
        7 => Opcode::StoreGlobal(r.str()?),
        8 => Opcode::PushScope,
        9 => Opcode::PopScope,
        10 => Opcode::Add,
        11 => Opcode::Sub,
        12 => Opcode::Mul,
        13 => Opcode::Div,
        14 => Opcode::Mod,
        15 => Opcode::Pow,
        16 => Opcode::Neg,
        17 => Opcode::Pos,
        18 => Opcode::Not,
        19 => Opcode::Eq,
        20 => Opcode::Neq,
        21 => Opcode::RawEq,
        22 => Opcode::Gt,
        23 => Opcode::Lt,
        24 => Opcode::Gte,
        25 => Opcode::Lte,
        26 => Opcode::And,
        27 => Opcode::Or,
        28 => Opcode::Pop,
        29 => Opcode::Dup,
        30 => Opcode::Swap,
        31 => Opcode::Jump(r.usize_()?),
        32 => Opcode::JumpIfTrue(r.usize_()?),
        33 => Opcode::JumpIfFalse(r.usize_()?),
        34 => {
            let catch = r.opt_usize()?;
            let finally = r.opt_usize()?;
            Opcode::TryPush { catch, finally }
        }
        35 => Opcode::TryPop,
        36 => Opcode::FinallyEnd,
        37 => Opcode::Call(r.usize_()?),
        38 => {
            let name = r.str()?;
            let argc = r.usize_()?;
            Opcode::CallNative(name, argc)
        }
        39 => Opcode::Return,
        40 => {
            let function_index = r.usize_()?;
            let captured = r.vec(|r| {
                let name = r.str()?;
                let is_const = r.bool()?;
                Ok((name, is_const))
            })?;
            Opcode::MakeClosure(function_index, captured)
        }
        41 => Opcode::LoadCaptured(r.str()?),
        42 => Opcode::StoreCaptured(r.str()?),
        43 => Opcode::Await,
        44 => Opcode::MakeList(r.usize_()?),
        45 => Opcode::MakeObject(r.vec(|r| r.str())?),
        46 => Opcode::ListAdd(r.usize_()?),
        47 => Opcode::ListRemove(r.usize_()?),
        48 => Opcode::ObjectRemove(r.usize_()?),
        49 => Opcode::GetIndex,
        50 => Opcode::SetIndex,
        51 => Opcode::GetProperty(r.str()?),
        52 => Opcode::SetProperty(r.str()?),
        53 => Opcode::Print,
        54 => Opcode::Halt,
        55 => Opcode::EndModule,
        56 => Opcode::Unknown(r.u64()?),
        other => Opcode::Unknown(other as u64),
    };
    Ok(op)
}

fn get_constant(r: &mut Reader) -> Result<Constant, BytecodeFileError> {
    let c = match r.u32()? {
        0 => Constant::Number(r.f64()?),
        1 => Constant::Str(r.str()?),
        2 => Constant::Boolean(r.bool()?),
        3 => Constant::Null,
        4 => Constant::Function(r.usize_()?),
        other => {
            return Err(BytecodeFileError::Malformed(format!(
                "unknown constant tag {}",
                other
            )))
        }
    };
    Ok(c)
}

fn get_function_info(r: &mut Reader) -> Result<FunctionInfo, BytecodeFileError> {
    let name = r.str()?;
    let params = r.vec(|r| r.str())?;
    let param_defaults = r.vec(|r| r.opt_usize())?;
    let entry_point = r.usize_()?;
    let local_count = r.usize_()?;
    let captured_vars = r.vec(|r| r.str())?;
    let is_async = r.bool()?;
    Ok(FunctionInfo {
        name,
        params,
        param_defaults,
        entry_point,
        local_count,
        captured_vars,
        is_async,
    })
}

fn get_module_meta(r: &mut Reader) -> Result<ModuleMeta, BytecodeFileError> {
    let path = r.str()?;
    let start = r.usize_()?;
    let end = r.usize_()?;
    let exports = r.vec(|r| r.str())?;
    let imports = r.vec(|r| {
        let local = r.str()?;
        let dep = r.str()?;
        let export = r.str()?;
        Ok((local, dep, export))
    })?;
    let re_exports = r.vec(|r| {
        let export = r.str()?;
        let dep = r.str()?;
        Ok((export, dep))
    })?;
    Ok(ModuleMeta {
        path,
        start,
        end,
        exports,
        imports,
        re_exports,
    })
}

fn get_bundle(r: &mut Reader) -> Result<ModuleBundle, BytecodeFileError> {
    let magic = r.str()?;
    let version = r.u32()?;
    let entry_point = r.usize_()?;
    let constants = r.vec(get_constant)?;
    let instructions = r.vec(get_opcode)?;
    let functions = r.vec(get_function_info)?;
    let function_bodies = r.vec(|r| r.vec(get_opcode))?;
    let default_bodies = r.vec(|r| r.vec(get_opcode))?;
    let modules = r.vec(get_module_meta)?;
    Ok(ModuleBundle {
        program: BytecodeProgram {
            header: BytecodeHeader { magic, version },
            entry_point,
            constants,
            instructions,
            functions,
            function_bodies,
            default_bodies,
        },
        modules,
    })
}

/// Verify the trailing SHA-256 checksum, then parse the payload (§158).
///
/// Order matters: integrity first (a corrupted file never reaches the
/// parser), compatibility second — magic/version inside the payload are
/// still checked by the VM's §124/§125 validation when the program runs.
pub fn load(bytes: &[u8]) -> Result<ModuleBundle, BytecodeFileError> {
    if bytes.len() < CHECKSUM_LEN {
        return Err(BytecodeFileError::Truncated);
    }
    let (payload, checksum) = bytes.split_at(bytes.len() - CHECKSUM_LEN);
    let mut hasher = Sha256::new();
    hasher.update(payload);
    if hasher.finish()[..] != *checksum {
        return Err(BytecodeFileError::BadChecksum);
    }
    let mut reader = Reader::new(payload);
    let bundle = get_bundle(&mut reader)?;
    if reader.remaining() != 0 {
        return Err(BytecodeFileError::Malformed(
            "trailing bytes after payload".to_string(),
        ));
    }
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vm::{VmError, ZeroVM};

    fn sample_bundle() -> ModuleBundle {
        let all_ops = vec![
            Opcode::LoadConst(7),
            Opcode::LoadFunction(2),
            Opcode::DeclareLocal("a".into(), true),
            Opcode::LoadLocal("b".into()),
            Opcode::StoreLocal("c".into()),
            Opcode::DeclareGlobal("g".into(), false),
            Opcode::LoadGlobal("h".into()),
            Opcode::StoreGlobal("i".into()),
            Opcode::PushScope,
            Opcode::PopScope,
            Opcode::Add,
            Opcode::Sub,
            Opcode::Mul,
            Opcode::Div,
            Opcode::Mod,
            Opcode::Pow,
            Opcode::Neg,
            Opcode::Pos,
            Opcode::Not,
            Opcode::Eq,
            Opcode::Neq,
            Opcode::RawEq,
            Opcode::Gt,
            Opcode::Lt,
            Opcode::Gte,
            Opcode::Lte,
            Opcode::And,
            Opcode::Or,
            Opcode::Pop,
            Opcode::Dup,
            Opcode::Swap,
            Opcode::Jump(3),
            Opcode::JumpIfTrue(4),
            Opcode::JumpIfFalse(5),
            Opcode::TryPush {
                catch: Some(6),
                finally: Some(7),
            },
            Opcode::TryPush {
                catch: None,
                finally: None,
            },
            Opcode::TryPop,
            Opcode::FinallyEnd,
            Opcode::Call(8),
            Opcode::CallNative("print".into(), 2),
            Opcode::Return,
            Opcode::MakeClosure(1, vec![("x".into(), true)]),
            Opcode::LoadCaptured("y".into()),
            Opcode::StoreCaptured("z".into()),
            Opcode::Await,
            Opcode::MakeList(3),
            Opcode::MakeObject(vec!["k1".into(), "k2".into()]),
            Opcode::ListAdd(1),
            Opcode::ListRemove(1),
            Opcode::ObjectRemove(1),
            Opcode::GetIndex,
            Opcode::SetIndex,
            Opcode::GetProperty("p".into()),
            Opcode::SetProperty("q".into()),
            Opcode::Print,
            Opcode::Halt,
            Opcode::EndModule,
            Opcode::Unknown(999),
        ];
        // 57 variants + TryPush exercised a second time with None/None.
        assert_eq!(all_ops.len(), 58, "all Opcode variants must be covered");
        ModuleBundle {
            program: BytecodeProgram {
                header: BytecodeHeader::current(),
                entry_point: 42,
                constants: vec![
                    Constant::Number(1.5),
                    Constant::Str("s".into()),
                    Constant::Boolean(true),
                    Constant::Null,
                    Constant::Function(0),
                ],
                instructions: all_ops.clone(),
                functions: vec![FunctionInfo {
                    name: "f".into(),
                    params: vec!["p0".into(), "p1".into()],
                    param_defaults: vec![Some(3), None],
                    entry_point: 11,
                    local_count: 4,
                    captured_vars: vec!["cap".into()],
                    is_async: true,
                }],
                function_bodies: vec![all_ops.clone()],
                default_bodies: vec![all_ops.clone()],
            },
            modules: vec![ModuleMeta {
                path: "/tmp/main.zs".into(),
                start: 0,
                end: 57,
                exports: vec!["e".into()],
                imports: vec![("l".into(), "/tmp/util.zs".into(), "e".into())],
                re_exports: vec![("re".into(), "/tmp/util.zs".into())],
            }],
        }
    }

    #[test]
    fn test_job024_roundtrip_all_opcode_variants_and_structures() {
        let bundle = sample_bundle();
        let bytes = save(&bundle);
        let loaded = load(&bytes).expect("roundtrip must load");
        assert_eq!(loaded.program.header, bundle.program.header);
        assert_eq!(loaded.program.entry_point, bundle.program.entry_point);
        assert_eq!(loaded.program.constants, bundle.program.constants);
        assert_eq!(loaded.program.instructions, bundle.program.instructions);
        assert_eq!(
            format!("{:?}", loaded.program.functions),
            format!("{:?}", bundle.program.functions)
        );
        assert_eq!(
            loaded.program.function_bodies,
            bundle.program.function_bodies
        );
        assert_eq!(loaded.program.default_bodies, bundle.program.default_bodies);
        assert_eq!(
            format!("{:?}", loaded.modules),
            format!("{:?}", bundle.modules)
        );
    }

    #[test]
    fn test_job024_bitflip_in_payload_fails_checksum() {
        let bundle = sample_bundle();
        let mut bytes = save(&bundle);
        let mid = bytes.len() / 2;
        bytes[mid] ^= 0x01;
        assert_eq!(load(&bytes).err(), Some(BytecodeFileError::BadChecksum));
    }

    #[test]
    fn test_job024_truncated_files_rejected() {
        let bytes = save(&sample_bundle());
        // Too short to even hold the 32-byte checksum.
        assert_eq!(load(&bytes[..10]).err(), Some(BytecodeFileError::Truncated));
        // Cut in half: the trailing 32 bytes are no longer the payload hash.
        assert_eq!(
            load(&bytes[..bytes.len() / 2]).err(),
            Some(BytecodeFileError::BadChecksum)
        );
    }

    #[test]
    fn test_job024_trailing_payload_bytes_rejected() {
        let bundle = sample_bundle();
        let mut payload = Vec::new();
        put_bundle(&mut payload, &bundle);
        payload.push(0); // stray byte before the hash
        let mut hasher = Sha256::new();
        hasher.update(&payload);
        payload.extend_from_slice(&hasher.finish());
        match load(&payload) {
            Err(BytecodeFileError::Malformed(detail)) => {
                assert!(detail.contains("trailing"), "{}", detail)
            }
            other => panic!("expected Malformed, got {:?}", other),
        }
    }

    /// A tag this build does not know must decode to `Opcode::Unknown` (so
    /// §124 rejects it as a VM Error), never be silently dropped or fatal
    /// at load time (ISSUES §68e, §158 checksum-vs-validation split).
    #[test]
    fn test_job024_foreign_opcode_tag_becomes_unknown_and_fails_validation() {
        let mut payload = Vec::new();
        put_str(&mut payload, "ZERO");
        put_u32(&mut payload, 1);
        put_usize(&mut payload, 0);
        put_vec(&mut payload, &[] as &[u32], |_, _| {});
        put_u64(&mut payload, 1); // one instruction
        put_u32(&mut payload, 99); // foreign opcode tag
        put_vec(&mut payload, &[] as &[u32], |_, _| {});
        put_vec(&mut payload, &[] as &[u32], |_, _| {});
        put_vec(&mut payload, &[] as &[u32], |_, _| {});
        put_vec(&mut payload, &[] as &[u32], |_, _| {});
        let mut file = payload;
        let mut hasher = Sha256::new();
        hasher.update(&file);
        file.extend_from_slice(&hasher.finish());

        let loaded = load(&file).expect("checksum and structure are valid");
        assert_eq!(loaded.program.instructions, vec![Opcode::Unknown(99)]);

        // §124: unknown opcode → VM Error, nothing executed.
        let mut vm = ZeroVM::new(loaded.program);
        match vm.run() {
            Err(VmError::InvalidOpcode(_)) => {}
            other => panic!("expected InvalidOpcode VM Error, got {:?}", other),
        }
        assert!(vm.get_output().is_empty());
    }

    /// §125: magic and version are compatibility concerns of the payload,
    /// verified at run time even when the checksum is intact.
    #[test]
    fn test_job024_bad_magic_and_version_rejected_at_run() {
        // Layout documented by put_bundle: u64 magic-len (0..8),
        // magic bytes (8..12), version u32 (12..16).
        let bytes = save(&sample_bundle());

        let mut bad_version = bytes.clone();
        bad_version[12..16].copy_from_slice(&999u32.to_le_bytes());
        rehash(&mut bad_version);
        let loaded = load(&bad_version).expect("intact checksum");
        match ZeroVM::new(loaded.program).run() {
            Err(VmError::UnsupportedBytecodeVersion(_)) => {}
            other => panic!("expected UnsupportedBytecodeVersion, got {:?}", other),
        }

        let mut bad_magic = bytes;
        bad_magic[8..12].copy_from_slice(b"XXXX");
        rehash(&mut bad_magic);
        let loaded = load(&bad_magic).expect("intact checksum");
        match ZeroVM::new(loaded.program).run() {
            Err(VmError::InvalidHeader(_)) => {}
            other => panic!("expected InvalidHeader, got {:?}", other),
        }
    }

    /// Re-derive the trailing checksum after a test patched the payload, so
    /// the file passes integrity exactly as a hostile-but-consistent artifact
    /// would.
    fn rehash(file: &mut Vec<u8>) {
        file.truncate(file.len() - CHECKSUM_LEN);
        let mut hasher = Sha256::new();
        hasher.update(file);
        file.extend_from_slice(&hasher.finish());
    }
}
