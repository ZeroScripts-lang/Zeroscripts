use crate::compiler::{BytecodeProgram, Opcode};

use super::{VmError, ZeroVM};

impl ZeroVM {
    /// Bytecode validation pass (§124): the first layer, run before the
    /// execution loop. A program that fails validation is never executed.
    ///
    /// Checks the header/format (§124, §125), opcode recognition, operand
    /// format, constant pool and function references, jump targets, function
    /// metadata (§143) and the program entry point — over the main instruction
    /// stream and over every function body.
    pub fn validate(&self) -> Result<(), VmError> {
        self.validate_header()?;

        let instruction_count = self.program.instructions.len();
        if instruction_count == 0 {
            if self.program.entry_point != 0 {
                return Err(VmError::InvalidBytecode(format!(
                    "Entry point {} outside empty program",
                    self.program.entry_point
                )));
            }
        } else if self.program.entry_point >= instruction_count {
            return Err(VmError::InvalidBytecode(format!(
                "Entry point {} outside program ({} instructions)",
                self.program.entry_point, instruction_count
            )));
        }

        Self::validate_stream(&self.program, &self.program.instructions)?;

        // §143: one body per function, each function's entry point covers the
        // copy of its body appended to the main stream, defaults stay in range.
        if self.program.functions.len() != self.program.function_bodies.len() {
            return Err(VmError::InvalidBytecode(format!(
                "{} function(s) but {} function bodie(s)",
                self.program.functions.len(),
                self.program.function_bodies.len()
            )));
        }

        for (index, function) in self.program.functions.iter().enumerate() {
            let body = &self.program.function_bodies[index];

            if function.params.len() != function.param_defaults.len() {
                return Err(VmError::InvalidOperand(format!(
                    "Function '{}' has {} parameter(s) but {} default(s)",
                    function.name,
                    function.params.len(),
                    function.param_defaults.len()
                )));
            }
            for default in function.param_defaults.iter().flatten() {
                if *default >= self.program.default_bodies.len() {
                    return Err(VmError::InvalidOperand(format!(
                        "Function '{}' default body index {} out of range",
                        function.name, *default
                    )));
                }
            }

            let body_end = function.entry_point.saturating_add(body.len());
            if function.entry_point > instruction_count || body_end > instruction_count {
                return Err(VmError::InvalidJumpTarget(function.entry_point));
            }

            Self::validate_stream(&self.program, body)?;
        }

        // Job 022 (§30): default-parameter bodies are standalone streams
        // (they run with pc = 0), validated like function bodies.
        for body in &self.program.default_bodies {
            Self::validate_stream(&self.program, body)?;
        }

        Ok(())
    }

    /// Header and version check (§124, §125). A bytecode version this ZeroVM
    /// does not support produces a VM Error before execution begins.
    fn validate_header(&self) -> Result<(), VmError> {
        if self.program.header.magic != crate::compiler::BYTECODE_MAGIC {
            return Err(VmError::InvalidHeader(format!(
                "expected magic '{}', found '{}'",
                crate::compiler::BYTECODE_MAGIC,
                self.program.header.magic
            )));
        }
        if !crate::compiler::SUPPORTED_BYTECODE_VERSIONS.contains(&self.program.header.version) {
            return Err(VmError::UnsupportedBytecodeVersion(
                self.program.header.version,
            ));
        }
        Ok(())
    }

    /// Per-instruction checks, shared by the main stream and by function
    /// bodies (body jumps are body-relative, so the stream length is bound).
    fn validate_stream(program: &BytecodeProgram, instructions: &[Opcode]) -> Result<(), VmError> {
        let constant_count = program.constants.len();
        let function_count = program.functions.len();
        let instruction_count = instructions.len();

        for instruction in instructions {
            match instruction {
                Opcode::LoadConst(idx) => {
                    if *idx >= constant_count {
                        return Err(VmError::InvalidConstantIndex(*idx));
                    }
                }
                Opcode::LoadFunction(idx) | Opcode::MakeClosure(idx, _) => {
                    if *idx >= function_count {
                        return Err(VmError::InvalidOperand(format!(
                            "Function index {} out of range",
                            idx
                        )));
                    }
                }
                Opcode::Jump(target)
                | Opcode::JumpIfTrue(target)
                | Opcode::JumpIfFalse(target) => {
                    if *target >= instruction_count {
                        return Err(VmError::InvalidJumpTarget(*target));
                    }
                }
                Opcode::TryPush { catch, finally } => {
                    if let Some(t) = catch {
                        if *t >= instruction_count {
                            return Err(VmError::InvalidJumpTarget(*t));
                        }
                    }
                    if let Some(t) = finally {
                        if *t >= instruction_count {
                            return Err(VmError::InvalidJumpTarget(*t));
                        }
                    }
                }
                Opcode::MakeObject(keys) => {
                    for key in keys {
                        if key.is_empty() {
                            return Err(VmError::InvalidOperand("Empty object key".to_string()));
                        }
                    }
                }
                Opcode::DeclareLocal(name, _)
                | Opcode::LoadLocal(name)
                | Opcode::StoreLocal(name)
                | Opcode::DeclareGlobal(name, _)
                | Opcode::LoadGlobal(name)
                | Opcode::StoreGlobal(name)
                | Opcode::LoadCaptured(name)
                | Opcode::StoreCaptured(name) => {
                    if name.is_empty() {
                        return Err(VmError::InvalidOperand("Empty variable name".to_string()));
                    }
                }
                Opcode::CallNative(operation, _) => {
                    if operation.is_empty() {
                        return Err(VmError::InvalidOperand(
                            "Empty native operation name".to_string(),
                        ));
                    }
                }
                Opcode::Unknown(tag) => {
                    return Err(VmError::InvalidOpcode(format!("Unknown opcode {}", tag)));
                }
                _ => {}
            }
        }

        Ok(())
    }

}
