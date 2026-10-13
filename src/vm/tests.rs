    use super::*;
    use crate::compiler::BytecodeHeader;
    use crate::compiler::{compile, Constant, FunctionInfo, Opcode};
    use crate::lexer::Lexer;
    use crate::parser::Parser;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    fn compile_source(source: &str) -> BytecodeProgram {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse().unwrap();
        compile(&program).unwrap()
    }

    fn run_source(source: &str) -> Result<Vec<String>, VmError> {
        let bc = compile_source(source);
        run_bytecode(bc)
    }

    /// Job 020 helper: run `source` with a GC threshold so collection
    /// actually fires at instruction boundaries. Returns
    /// (output, collections run, objects left on the heap).
    fn run_source_with_gc_pressure(
        source: &str,
        threshold: usize,
    ) -> Result<(Vec<String>, usize, usize), VmError> {
        let bc = compile_source(source);
        let mut vm = ZeroVM::new(bc);
        vm.gc = Gc::new(threshold);
        vm.run()?;
        Ok((
            vm.get_output().to_vec(),
            vm.gc.stats().collections,
            vm.heap.count(),
        ))
    }

    #[test]
    fn test_empty_program() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![],
            instructions: vec![Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(*vm.get_state(), ExecutionState::Completed);
    }

    #[test]
    fn test_validate_empty() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![],
            instructions: vec![],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let vm = ZeroVM::new(bc);
        assert!(vm.validate().is_ok());
    }

    #[test]
    fn test_validate_invalid_constant_index() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(5)],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let vm = ZeroVM::new(bc);
        assert!(vm.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_jump_target() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![],
            instructions: vec![Opcode::Jump(10)],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let vm = ZeroVM::new(bc);
        assert!(vm.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_function_entry() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![],
            instructions: vec![Opcode::Halt],
            functions: vec![FunctionInfo {
                name: "test".to_string(),
                params: vec![],
                param_defaults: vec![],
                entry_point: 10,
                local_count: 0,
                captured_vars: vec![],
                is_async: false,
            }],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let vm = ZeroVM::new(bc);
        assert!(vm.validate().is_err());
    }

    #[test]
    fn test_validate_valid() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Pop, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let vm = ZeroVM::new(bc);
        assert!(vm.validate().is_ok());
    }

    #[test]
    fn test_stack_push_pop() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(42.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack().len(), 1);
        assert_eq!(vm.get_stack()[0], Value::Number(42.0));
    }

    #[test]
    fn test_stack_underflow_pop() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![],
            instructions: vec![Opcode::Pop],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        let result = vm.run();
        assert!(result.is_err());
    }

    #[test]
    fn test_stack_dup() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Dup, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack().len(), 2);
        assert_eq!(vm.get_stack()[0], Value::Number(10.0));
        assert_eq!(vm.get_stack()[1], Value::Number(10.0));
    }

    #[test]
    fn test_stack_swap() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(20.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Swap,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(20.0));
        assert_eq!(vm.get_stack()[1], Value::Number(10.0));
    }

    #[test]
    fn test_stack_swap_underflow() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Swap],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_constant_access() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![
                Constant::Number(10.0),
                Constant::Str("hello".to_string()),
                Constant::Boolean(true),
                Constant::Null,
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::LoadConst(2),
                Opcode::LoadConst(3),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(10.0));
        assert_eq!(vm.get_stack()[1], Value::Str("hello".to_string()));
        assert_eq!(vm.get_stack()[2], Value::Boolean(true));
        assert_eq!(vm.get_stack()[3], Value::Null);
    }

    #[test]
    fn test_invalid_constant_index_runtime() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(5)],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_add_numbers() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(20.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Add,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(30.0));
    }

    #[test]
    fn test_subtract_numbers() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(3.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Sub,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(7.0));
    }

    #[test]
    fn test_multiply_numbers() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(3.0), Constant::Number(4.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Mul,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(12.0));
    }

    #[test]
    fn test_divide_numbers() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(2.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Div,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(5.0));
    }

    #[test]
    fn test_divide_by_zero() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(0.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Div,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_modulo_numbers() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(3.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Mod,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(1.0));
    }

    #[test]
    fn test_modulo_by_zero() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(0.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Mod,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_power_numbers() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(2.0), Constant::Number(3.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Pow,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(8.0));
    }

    #[test]
    fn test_negate() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Neg, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(-10.0));
    }

    #[test]
    fn test_positive() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Pos, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(10.0));
    }

    #[test]
    fn test_not() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Boolean(true)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Not, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(false));
    }

    #[test]
    fn test_not_on_non_boolean() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Not],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_equality_equal() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(10.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Eq,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(true));
    }

    #[test]
    fn test_equality_not_equal() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(20.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Eq,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(false));
    }

    #[test]
    fn test_neq() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(20.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Neq,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(true));
    }

    #[test]
    fn test_strict_eq_same_type() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(10.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::RawEq,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(true));
    }

    #[test]
    fn test_strict_eq_different_type() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Str("10".to_string())],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::RawEq,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(true));
    }

    #[test]
    fn test_eq_different_type() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Str("10".to_string())],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Eq,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(false));
    }

    #[test]
    fn test_gt() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(5.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Gt,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(true));
    }

    #[test]
    fn test_lt() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(5.0), Constant::Number(10.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Lt,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(true));
    }

    #[test]
    fn test_gte() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(10.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Gte,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(true));
    }

    #[test]
    fn test_lte() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(5.0), Constant::Number(10.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Lte,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(true));
    }

    #[test]
    fn test_and() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Boolean(true), Constant::Boolean(false)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::And,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(false));
    }

    #[test]
    fn test_or() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Boolean(true), Constant::Boolean(false)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Or,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Boolean(true));
    }

    #[test]
    fn test_and_non_boolean() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Boolean(true)],
            instructions: vec![Opcode::LoadConst(0), Opcode::LoadConst(1), Opcode::And],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_jump() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Number(20.0)],
            instructions: vec![
                Opcode::Jump(2),
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(20.0));
    }

    #[test]
    fn test_jump_if_false() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![
                Constant::Boolean(false),
                Constant::Number(10.0),
                Constant::Number(20.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::JumpIfFalse(3),
                Opcode::LoadConst(1),
                Opcode::Pop,
                Opcode::LoadConst(2),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(20.0));
    }

    #[test]
    fn test_jump_if_true() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![
                Constant::Boolean(true),
                Constant::Number(10.0),
                Constant::Number(20.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::JumpIfTrue(3),
                Opcode::LoadConst(1),
                Opcode::Pop,
                Opcode::LoadConst(2),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(20.0));
    }

    #[test]
    fn test_jump_invalid_target() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![],
            instructions: vec![Opcode::Jump(10)],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_jump_if_false_non_boolean() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::JumpIfFalse(0)],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_variable_store_load() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(42.0)],
            instructions: vec![
                Opcode::DeclareLocal("A".into(), false),
                Opcode::LoadConst(0),
                Opcode::StoreLocal("A".into()),
                Opcode::LoadLocal("A".into()),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(42.0));
    }

    #[test]
    fn test_variable_undefined() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![],
            instructions: vec![Opcode::LoadLocal("X".into())],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_print() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(42.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Print, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_output(), &["42"]);
    }

    #[test]
    fn test_print_string() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Str("hello".to_string())],
            instructions: vec![Opcode::LoadConst(0), Opcode::Print, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_output(), &["hello"]);
    }

    #[test]
    fn test_halt() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![
                Constant::Number(10.0),
                Constant::Number(20.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::Halt,
                Opcode::LoadConst(1),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack().len(), 1);
        assert_eq!(vm.get_stack()[0], Value::Number(10.0));
    }

    #[test]
    fn test_make_list() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![
                Constant::Number(10.0),
                Constant::Number(20.0),
                Constant::Number(30.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::LoadConst(2),
                Opcode::MakeList(3),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        // List is heap-backed; check structure
        match &vm.get_stack()[0] {
            Value::List(r) => match vm.heap.get(r).unwrap() {
                HeapObject::List { items, .. } => {
                    assert_eq!(items.len(), 3);
                    assert_eq!(
                        Value::from_heap_ref(&items[0], &vm.heap).unwrap(),
                        Value::Number(10.0)
                    );
                    assert_eq!(
                        Value::from_heap_ref(&items[1], &vm.heap).unwrap(),
                        Value::Number(20.0)
                    );
                    assert_eq!(
                        Value::from_heap_ref(&items[2], &vm.heap).unwrap(),
                        Value::Number(30.0)
                    );
                }
                _ => panic!("Expected list on heap"),
            },
            other => panic!("Expected Value::List, got {:?}", other),
        }
    }

    #[test]
    fn test_make_empty_list() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![],
            instructions: vec![Opcode::MakeList(0), Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        match &vm.get_stack()[0] {
            Value::List(r) => match vm.heap.get(r).unwrap() {
                HeapObject::List { items, .. } => assert_eq!(items.len(), 0),
                _ => panic!("Expected list on heap"),
            },
            other => panic!("Expected Value::List, got {:?}", other),
        }
    }

    #[test]
    fn test_make_object() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![
                Constant::Str("Kael".to_string()),
                Constant::Number(18.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::MakeObject(vec!["name".into(), "age".into()]),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        match &vm.get_stack()[0] {
            Value::Object(r) => match vm.heap.get(r).unwrap() {
                HeapObject::Object { props, .. } => {
                    assert_eq!(props.len(), 2);
                    let name_ref = props.iter().find(|(k, _)| k == "name").unwrap().1.clone();
                    let age_ref = props.iter().find(|(k, _)| k == "age").unwrap().1.clone();
                    assert_eq!(
                        Value::from_heap_ref(&name_ref, &vm.heap).unwrap(),
                        Value::Str("Kael".to_string())
                    );
                    assert_eq!(
                        Value::from_heap_ref(&age_ref, &vm.heap).unwrap(),
                        Value::Number(18.0)
                    );
                }
                _ => panic!("Expected object on heap"),
            },
            other => panic!("Expected Value::Object, got {:?}", other),
        }
    }

    #[test]
    fn test_get_index_list() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![
                Constant::Number(10.0),
                Constant::Number(20.0),
                Constant::Number(0.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::MakeList(2),
                Opcode::LoadConst(2),
                Opcode::GetIndex,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(10.0));
    }

    #[test]
    fn test_get_index_out_of_bounds() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![
                Constant::Number(10.0),
                Constant::Number(5.0),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::MakeList(1),
                Opcode::LoadConst(1),
                Opcode::GetIndex,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_get_property() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Str("Kael".to_string())],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::MakeObject(vec!["name".into()]),
                Opcode::GetProperty("name".into()),
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Str("Kael".to_string()));
    }

    #[test]
    fn test_get_property_not_found() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Str("Kael".to_string())],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::MakeObject(vec!["name".into()]),
                Opcode::GetProperty("missing".into()),
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_add_strings() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![
                Constant::Str("Hello ".to_string()),
                Constant::Str("World".to_string()),
            ],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Add,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Str("Hello World".to_string()));
    }

    #[test]
    fn test_add_null_with_number() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Null, Constant::Number(10.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Add,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(10.0));
    }

    #[test]
    fn test_add_number_with_null() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0), Constant::Null],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Add,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(10.0));
    }

    #[test]
    fn test_add_null_null() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Null, Constant::Null],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(1),
                Opcode::Add,
                Opcode::Halt,
            ],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(0.0));
    }

    #[test]
    fn test_negate_null() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Null],
            instructions: vec![Opcode::LoadConst(0), Opcode::Neg, Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_stack()[0], Value::Number(0.0));
    }

    #[test]
    fn test_pc_out_of_bounds() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![],
            instructions: vec![Opcode::Jump(100)],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_call_non_function() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0), Opcode::Call(0)],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_call_wrong_arg_count() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(0.0)],
            instructions: vec![
                Opcode::LoadConst(0),
                Opcode::Call(0),
                Opcode::Halt,
            ],
            functions: vec![FunctionInfo {
                name: "test".to_string(),
                params: vec!["A".to_string()],
                param_defaults: vec![],
                entry_point: 3,
                local_count: 1,
                captured_vars: vec![],
                is_async: false,
            }],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(vm.run().is_err());
    }

    #[test]
    fn test_execution_state_running() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![Constant::Number(10.0)],
            instructions: vec![Opcode::LoadConst(0)],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let vm = ZeroVM::new(bc);
        assert_eq!(*vm.get_state(), ExecutionState::Running);
    }

    #[test]
    fn test_execution_completed() {
        let bc = BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0,
            constants: vec![],
            instructions: vec![Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(*vm.get_state(), ExecutionState::Completed);
    }

    #[test]
    fn test_value_display_number() {
        assert_eq!(format!("{}", Value::Number(42.0)), "42");
        assert_eq!(format!("{}", Value::Number(3.14)), "3.14");
    }

    #[test]
    fn test_value_display_string() {
        assert_eq!(
            format!("{}", Value::Str("hello".to_string())),
            "hello"
        );
    }

    #[test]
    fn test_value_display_boolean() {
        assert_eq!(format!("{}", Value::Boolean(true)), "true");
        assert_eq!(format!("{}", Value::Boolean(false)), "false");
    }

    #[test]
    fn test_value_display_null() {
        assert_eq!(format!("{}", Value::Null), "null");
    }

    #[test]
    fn test_value_display_list() {
        let mut heap = Heap::new();
        let n1 = heap.allocate(HeapObject::Number(1.0));
        let n2 = heap.allocate(HeapObject::Number(2.0));
        let n3 = heap.allocate(HeapObject::Number(3.0));
        let list_ref = heap.allocate(HeapObject::list(vec![n1, n2, n3]));
        let list = Value::List(list_ref);
        assert_eq!(list.format_value(&heap), "[1, 2, 3]");
        // Display without heap shows identity placeholder
        assert!(format!("{}", list).starts_with("<list "));
    }

    #[test]
    fn test_value_display_object() {
        let mut heap = Heap::new();
        let name_val = heap.allocate(HeapObject::Str("Kael".to_string()));
        let obj_ref = heap.allocate(HeapObject::object(vec![("name".to_string(), name_val)]));
        let obj = Value::Object(obj_ref);
        let formatted = obj.format_value(&heap);
        assert!(formatted.contains("name: Kael"));
    }

    #[test]
    fn test_value_display_function() {
        assert_eq!(format!("{}", Value::Function(0)), "<function 0>");
    }

    #[test]
    fn test_value_equality() {
        assert_eq!(Value::Number(10.0), Value::Number(10.0));
        assert_ne!(Value::Number(10.0), Value::Number(20.0));
        assert_eq!(
            Value::Str("hello".to_string()),
            Value::Str("hello".to_string())
        );
        assert_eq!(Value::Boolean(true), Value::Boolean(true));
        assert_eq!(Value::Null, Value::Null);
    }

    // Integration tests: full pipeline source -> lexer -> parser -> compiler -> VM

    #[test]
    fn test_integration_print_number() {
        let output = run_source("print(42)").unwrap();
        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn test_integration_print_string() {
        let output = run_source(r#"print("hello")"#).unwrap();
        assert_eq!(output, vec!["hello"]);
    }

    #[test]
    fn test_integration_print_boolean() {
        let output = run_source("print(true)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_print_null() {
        let output = run_source("print(null)").unwrap();
        assert_eq!(output, vec!["null"]);
    }

    #[test]
    fn test_integration_arithmetic() {
        let output = run_source("print(10 + 5)").unwrap();
        assert_eq!(output, vec!["15"]);
    }

    #[test]
    fn test_integration_subtract() {
        let output = run_source("print(10 - 3)").unwrap();
        assert_eq!(output, vec!["7"]);
    }

    #[test]
    fn test_integration_multiply() {
        let output = run_source("print(3 * 4)").unwrap();
        assert_eq!(output, vec!["12"]);
    }

    #[test]
    fn test_integration_divide() {
        let output = run_source("print(10 / 2)").unwrap();
        assert_eq!(output, vec!["5"]);
    }

    #[test]
    fn test_integration_modulo() {
        let output = run_source("print(10 % 3)").unwrap();
        assert_eq!(output, vec!["1"]);
    }

    #[test]
    fn test_integration_power() {
        let output = run_source("print(2 ** 3)").unwrap();
        assert_eq!(output, vec!["8"]);
    }

    #[test]
    fn test_integration_precedence() {
        let output = run_source("print(2 + 3 * 4)").unwrap();
        assert_eq!(output, vec!["14"]);
    }

    #[test]
    fn test_integration_string_concat() {
        let output = run_source(r#"print("Hello " + "World")"#).unwrap();
        assert_eq!(output, vec!["Hello World"]);
    }

    #[test]
    fn test_integration_equality() {
        let output = run_source("print(10 == 10)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_equality_false() {
        let output = run_source("print(10 == 20)").unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_integration_not_equal() {
        let output = run_source("print(10 != 20)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_strict_equality() {
        let output = run_source("print(10 === 10)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_strict_equality_cross_type() {
        let output = run_source(r#"print(10 === "10")"#).unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_equality_cross_type() {
        let output = run_source(r#"print(10 == "10")"#).unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_integration_comparison_gt() {
        let output = run_source("print(10 > 5)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_comparison_lt() {
        let output = run_source("print(5 < 10)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_comparison_gte() {
        let output = run_source("print(10 >= 10)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_comparison_lte() {
        let output = run_source("print(5 <= 10)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_boolean_and() {
        let output = run_source("print(true and false)").unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_integration_boolean_or() {
        let output = run_source("print(true or false)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_integration_not() {
        let output = run_source("print(!true)").unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_integration_negate() {
        let output = run_source("print(-10)").unwrap();
        assert_eq!(output, vec!["-10"]);
    }

    #[test]
    fn test_integration_variable_let() {
        let output = run_source("let A = 10\nprint(A)").unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_integration_variable_assign() {
        let output = run_source("let A = 10\nA = 20\nprint(A)").unwrap();
        assert_eq!(output, vec!["20"]);
    }

    #[test]
    fn test_integration_compound_assignment() {
        let output = run_source("let A = 10\nA += 5\nprint(A)").unwrap();
        assert_eq!(output, vec!["15"]);
    }

    #[test]
    fn test_integration_if_else() {
        let output = run_source("if false { print(10) } else { print(20) }").unwrap();
        assert_eq!(output, vec!["20"]);
    }

    #[test]
    fn test_integration_while_loop() {
        let output = run_source("let A = 0\nwhile A < 3 {\n    A += 1\n    print(A)\n}").unwrap();
        assert_eq!(output, vec!["1", "2", "3"]);
    }

    #[test]
    fn test_integration_loop_break() {
        let output = run_source("let A = 0\nloop {\n    A += 1\n    if A == 3 { break }\n    print(A)\n}").unwrap();
        assert_eq!(output, vec!["1", "2"]);
    }

    #[test]
    fn test_integration_list() {
        let output = run_source("let A = [10, 20, 30]\nprint(A[0])\nprint(A[1])\nprint(A[2])").unwrap();
        assert_eq!(output, vec!["10", "20", "30"]);
    }

    #[test]
    fn test_integration_object_access() {
        let output = run_source("let user = { name: \"Kael\" }\nprint(user.name)").unwrap();
        assert_eq!(output, vec!["Kael"]);
    }

    #[test]
    fn test_integration_function_declaration_and_call() {
        let source = "function tambah(A, B) {\n    return A + B\n}\nprint(tambah(10, 20))";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["30"]);
    }

    #[test]
    fn test_integration_function_no_return() {
        let source = "function test() {\n}\nprint(test())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["null"]);
    }

    #[test]
    fn test_integration_function_bare_return() {
        let source = "function test() {\n    return\n}\nprint(test())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["null"]);
    }

    #[test]
    fn test_integration_division_by_zero() {
        let result = run_source("print(10 / 0)");
        assert!(result.is_err());
    }

    #[test]
    fn test_integration_string_index() {
        let output = run_source("let s = \"Hello\"\nprint(s[0])").unwrap();
        assert_eq!(output, vec!["H"]);
    }

    #[test]
    fn test_integration_postfix_increment() {
        let output = run_source("let A = 10\nprint(A++)\nprint(A)").unwrap();
        assert_eq!(output, vec!["10", "11"]);
    }

    #[test]
    fn test_integration_postfix_decrement() {
        let output = run_source("let A = 10\nprint(A--)\nprint(A)").unwrap();
        assert_eq!(output, vec!["10", "9"]);
    }

    #[test]
    fn test_integration_compound_assignment_subtract() {
        let output = run_source("let A = 10\nA -= 3\nprint(A)").unwrap();
        assert_eq!(output, vec!["7"]);
    }

    #[test]
    fn test_integration_compound_assignment_multiply() {
        let output = run_source("let A = 5\nA *= 3\nprint(A)").unwrap();
        assert_eq!(output, vec!["15"]);
    }

    #[test]
    fn test_integration_compound_assignment_divide() {
        let output = run_source("let A = 10\nA /= 2\nprint(A)").unwrap();
        assert_eq!(output, vec!["5"]);
    }

    #[test]
    fn test_integration_compound_assignment_modulo() {
        let output = run_source("let A = 10\nA %= 3\nprint(A)").unwrap();
        assert_eq!(output, vec!["1"]);
    }

    // ===== Heap-backed runtime value integration tests =====

    #[test]
    fn test_runtime_value_round_trip_number() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let rv = Value::Number(42.5).to_runtime_value(&mut vm.heap);
        let back = Value::from_runtime_value(&rv, &vm.heap).unwrap();
        assert_eq!(back, Value::Number(42.5));
    }

    #[test]
    fn test_runtime_value_round_trip_boolean() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let rv = Value::Boolean(true).to_runtime_value(&mut vm.heap);
        let back = Value::from_runtime_value(&rv, &vm.heap).unwrap();
        assert_eq!(back, Value::Boolean(true));
    }

    #[test]
    fn test_runtime_value_round_trip_null() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let rv = Value::Null.to_runtime_value(&mut vm.heap);
        let back = Value::from_runtime_value(&rv, &vm.heap).unwrap();
        assert_eq!(back, Value::Null);
    }

    #[test]
    fn test_runtime_value_round_trip_string() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let rv = Value::Str("hello heap".to_string()).to_runtime_value(&mut vm.heap);
        let back = Value::from_runtime_value(&rv, &vm.heap).unwrap();
        assert_eq!(back, Value::Str("hello heap".to_string()));
    }

    #[test]
    fn test_runtime_value_round_trip_list() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        // Build a heap list
        let n = vm.heap.allocate(HeapObject::Number(1.0));
        let s = vm.heap.allocate(HeapObject::Str("two".to_string()));
        let b = vm.heap.allocate(HeapObject::Boolean(false));
        let z = vm.heap.allocate(HeapObject::Null);
        let list_ref = vm.heap.allocate(HeapObject::list(vec![n, s, b, z]));
        let list = Value::List(list_ref.clone());
        let rv = list.to_runtime_value(&mut vm.heap);
        let back = Value::from_runtime_value(&rv, &vm.heap).unwrap();
        assert_eq!(back, list);
        // Shared identity: same handle
        match back {
            Value::List(r) => assert_eq!(r.index(), list_ref.index()),
            _ => panic!("Expected list"),
        }
    }

    #[test]
    fn test_runtime_value_round_trip_object() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let x = vm.heap.allocate(HeapObject::Number(10.0));
        let y = vm.heap.allocate(HeapObject::Str("hello".to_string()));
        let obj_ref = vm.heap.allocate(HeapObject::object(vec![
            ("x".to_string(), x),
            ("y".to_string(), y),
        ]));
        let obj = Value::Object(obj_ref.clone());
        let rv = obj.to_runtime_value(&mut vm.heap);
        let back = Value::from_runtime_value(&rv, &vm.heap).unwrap();
        match back {
            Value::Object(back_ref) => {
                assert_eq!(back_ref.index(), obj_ref.index());
                match vm.heap.get(&back_ref).unwrap() {
                    HeapObject::Object { props, .. } => {
                        assert_eq!(props.len(), 2);
                    }
                    _ => panic!("Expected object on heap"),
                }
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_heap_invalidation() {
        let mut heap = Heap::new();
        let r1 = heap.allocate(HeapObject::Str("alive".to_string()));
        assert!(heap.is_valid(&r1));

        // Allocate more to push r1 into free list
        let mut refs = Vec::new();
        for _ in 0..10 {
            refs.push(heap.allocate(HeapObject::Str("filler".to_string())));
        }

        // Deallocate r1, then reallocate - generation should change
        heap.deallocate(&r1).unwrap();
        assert!(!heap.is_valid(&r1));

        let r2 = heap.allocate(HeapObject::Str("reused".to_string()));
        // r2 should have index of r1 but different generation
        assert_eq!(r2.index(), r1.index());
        assert_ne!(r2.generation(), r1.generation());
        assert!(heap.is_valid(&r2));
    }

    #[test]
    fn test_heap_multiple_allocations() {
        let mut heap = Heap::new();
        let r1 = heap.allocate(HeapObject::Str("first".to_string()));
        let r2 = heap.allocate(HeapObject::Str("second".to_string()));
        let r3 = heap.allocate(HeapObject::Str("third".to_string()));

        assert_eq!(r1.index(), 0);
        assert_eq!(r2.index(), 1);
        assert_eq!(r3.index(), 2);

        match heap.get(&r1).unwrap() {
            HeapObject::Str(s) => assert_eq!(s, "first"),
            _ => panic!("Expected Str"),
        }
        match heap.get(&r2).unwrap() {
            HeapObject::Str(s) => assert_eq!(s, "second"),
            _ => panic!("Expected Str"),
        }
        match heap.get(&r3).unwrap() {
            HeapObject::Str(s) => assert_eq!(s, "third"),
            _ => panic!("Expected Str"),
        }
    }

    #[test]
    fn test_heap_free_list_reuse() {
        let mut heap = Heap::new();
        let r1 = heap.allocate(HeapObject::Str("a".to_string()));
        let r2 = heap.allocate(HeapObject::Str("b".to_string()));
        assert_eq!(r1.index(), 0);
        assert_eq!(r2.index(), 1);

        heap.deallocate(&r1).unwrap();

        let r3 = heap.allocate(HeapObject::Str("c".to_string()));
        // r3 should reuse r1's slot
        assert_eq!(r3.index(), r1.index());
        assert_ne!(r3.generation(), r1.generation());
    }

    #[test]
    fn test_heap_contains() {
        let mut heap = Heap::new();
        let r1 = heap.allocate(HeapObject::Str("test".to_string()));
        assert!(heap.contains(&r1));
        heap.deallocate(&r1).unwrap();
        assert!(!heap.contains(&r1));
    }

    #[test]
    fn test_heap_list_allocation() {
        let mut heap = Heap::new();
        let e1 = heap.allocate(HeapObject::Str("a".to_string()));
        let e2 = heap.allocate(HeapObject::Str("b".to_string()));
        let e3 = heap.allocate(HeapObject::Str("c".to_string()));
        let list_ref = heap.allocate(HeapObject::list(vec![e1, e2, e3]));
        match heap.get(&list_ref).unwrap() {
            HeapObject::List { items, .. } => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn test_heap_object_allocation() {
        let mut heap = Heap::new();
        let v0 = heap.allocate(HeapObject::Str("10".to_string()));
        let v1 = heap.allocate(HeapObject::Str("20".to_string()));
        let obj_ref = heap.allocate(HeapObject::object(vec![
            ("x".to_string(), v0),
            ("y".to_string(), v1),
        ]));
        match heap.get(&obj_ref).unwrap() {
            HeapObject::Object { props, .. } => {
                assert_eq!(props.len(), 2);
                assert_eq!(props[0].0, "x");
                assert_eq!(props[1].0, "y");
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_heap_function_allocation() {
        let mut heap = Heap::new();
        let func_ref = heap.allocate(HeapObject::Function {
            index: 0,
            name: "test_func".to_string(),
            params: vec!["a".to_string(), "b".to_string()],
            entry_point: 42,
        });
        match heap.get(&func_ref).unwrap() {
            HeapObject::Function {
                index,
                name,
                params,
                entry_point,
            } => {
                assert_eq!(*index, 0);
                assert_eq!(name, "test_func");
                assert_eq!(params, &vec!["a".to_string(), "b".to_string()]);
                assert_eq!(*entry_point, 42);
            }
            _ => panic!("Expected Function"),
        }
    }

    // ===== GC integration tests =====

    #[test]
    fn test_gc_manual_collect() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let r1 = vm.heap.allocate(HeapObject::Str("keep".to_string()));
        let r2 = vm.heap.allocate(HeapObject::Str("drop".to_string()));
        vm.add_live_ref(r1.clone());

        vm.run_gc();

        assert!(vm.heap.is_valid(&r1));
        assert!(!vm.heap.is_valid(&r2));
    }

    #[test]
    fn test_gc_collect_reachable_list_elements() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let elem = vm.heap.allocate(HeapObject::Str("elem".to_string()));
        let list = vm.heap.allocate(HeapObject::list(vec![elem.clone()]));
        let orphan = vm.heap.allocate(HeapObject::Str("orphan".to_string()));

        vm.add_live_ref(list.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&list));
        assert!(vm.heap.is_valid(&elem));
        assert!(!vm.heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_collect_reachable_object_values() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let val = vm.heap.allocate(HeapObject::Str("val".to_string()));
        let obj = vm.heap.allocate(HeapObject::object(vec![("key".to_string(), val.clone())]));
        let orphan = vm.heap.allocate(HeapObject::Str("orphan".to_string()));

        vm.add_live_ref(obj.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&obj));
        assert!(vm.heap.is_valid(&val));
        assert!(!vm.heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_collect_unreachable_cycle() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let a = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let b = vm.heap.allocate(HeapObject::Str("b".to_string()));

        *vm.heap.get_mut(&a).unwrap() = HeapObject::list(vec![b.clone()]);
        *vm.heap.get_mut(&b).unwrap() = HeapObject::list(vec![a.clone()]);

        vm.run_gc();

        assert!(!vm.heap.is_valid(&a));
        assert!(!vm.heap.is_valid(&b));
    }

    #[test]
    fn test_gc_reachable_cycle_survives() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let a = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let b = vm.heap.allocate(HeapObject::Str("b".to_string()));

        *vm.heap.get_mut(&a).unwrap() = HeapObject::list(vec![b.clone()]);
        *vm.heap.get_mut(&b).unwrap() = HeapObject::list(vec![a.clone()]);

        vm.add_live_ref(a.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&a));
        assert!(vm.heap.is_valid(&b));
    }

    #[test]
    fn test_gc_stale_refs_cleaned_after_collect() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let r1 = vm.heap.allocate(HeapObject::Str("temp".to_string()));
        vm.add_live_ref(r1.clone());

        vm.run_gc();

        assert!(vm.live_refs().iter().all(|r| vm.heap.is_valid(r)));
    }

    #[test]
    fn test_gc_trigger_on_threshold() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        vm.gc = Gc::new(3);

        let r1 = vm.heap.allocate(HeapObject::Str("a".to_string()));
        vm.add_live_ref(r1.clone());
        vm.gc.on_allocation();

        let r2 = vm.heap.allocate(HeapObject::Str("b".to_string()));
        vm.add_live_ref(r2.clone());
        vm.gc.on_allocation();

        let r3 = vm.heap.allocate(HeapObject::Str("c".to_string()));
        vm.add_live_ref(r3.clone());
        vm.gc.on_allocation();

        assert_eq!(vm.gc.stats().collections, 0);

        let r4 = vm.heap.allocate(HeapObject::Str("d".to_string()));
        vm.add_live_ref(r4.clone());
        vm.gc.on_allocation();
        vm.maybe_gc();

        assert_eq!(vm.gc.stats().collections, 1);
    }

    #[test]
    fn test_gc_diamond_graph_via_vm() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let shared = vm.heap.allocate(HeapObject::Str("shared".to_string()));
        let left = vm.heap.allocate(HeapObject::list(vec![shared.clone()]));
        let right = vm.heap.allocate(HeapObject::list(vec![shared.clone()]));
        let root = vm.heap.allocate(HeapObject::list(vec![left.clone(), right.clone()]));
        let orphan = vm.heap.allocate(HeapObject::Str("orphan".to_string()));

        vm.add_live_ref(root.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&root));
        assert!(vm.heap.is_valid(&left));
        assert!(vm.heap.is_valid(&right));
        assert!(vm.heap.is_valid(&shared));
        assert!(!vm.heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_multiple_live_refs() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let r1 = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let r2 = vm.heap.allocate(HeapObject::Str("b".to_string()));
        let orphan = vm.heap.allocate(HeapObject::Str("orphan".to_string()));

        vm.add_live_ref(r1.clone());
        vm.add_live_ref(r2.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&r1));
        assert!(vm.heap.is_valid(&r2));
        assert!(!vm.heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_stats_tracking() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        let _r1 = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let _r2 = vm.heap.allocate(HeapObject::Str("b".to_string()));

        vm.run_gc();

        assert_eq!(vm.gc().stats().collections, 1);
        assert_eq!(vm.gc().stats().objects_freed, 2);
        assert_eq!(vm.gc().stats().objects_survived, 0);
    }

    #[test]
    fn test_gc_integration_print_function() {
        let output = run_source("print(\"hello\")").unwrap();
        assert_eq!(output, vec!["hello"]);
    }

    #[test]
    fn test_gc_integration_list_operations() {
        let output = run_source("let L = [1, 2, 3]\nprint(L[0])\nprint(L[1])").unwrap();
        assert_eq!(output, vec!["1", "2"]);
    }

    #[test]
    fn test_gc_integration_object_operations() {
        let output = run_source("let O = { x: 10 }\nprint(O.x)").unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_gc_integration_function_calls() {
        let source = "function add(A, B) {\n    return A + B\n}\nprint(add(3, 4))";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["7"]);
    }

    #[test]
    fn test_gc_integration_string_concat() {
        let output = run_source(r#"print("Hello " + "World")"#).unwrap();
        assert_eq!(output, vec!["Hello World"]);
    }

    #[test]
    fn test_gc_automatic_trigger_during_execution() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        vm.gc = Gc::new(5);

        for i in 0..20 {
            let val = vm.heap.allocate(HeapObject::Str(format!("str_{}", i)));
            vm.add_live_ref(val);
            vm.gc.on_allocation();
            vm.maybe_gc();
        }

        assert!(vm.gc.stats().collections > 0);
        assert_eq!(vm.gc.stats().allocations_since_gc, 0);
    }

    #[test]
    fn test_gc_automatic_source_execution() {
        let source = "let A = \"first\"\nlet B = \"second\"\nlet C = \"third\"\nprint(A)\nprint(B)\nprint(C)";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["first", "second", "third"]);
    }

    #[test]
    fn test_gc_reachable_objects_survive() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        vm.gc = Gc::new(0);

        let r1 = vm.heap.allocate(HeapObject::Str("keep1".to_string()));
        let r2 = vm.heap.allocate(HeapObject::Str("keep2".to_string()));
        vm.add_live_ref(r1.clone());
        vm.add_live_ref(r2.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&r1));
        assert!(vm.heap.is_valid(&r2));
    }

    #[test]
    fn test_gc_unreachable_objects_collected() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        vm.gc = Gc::new(0);

        let _r1 = vm.heap.allocate(HeapObject::Str("orphan1".to_string()));
        let _r2 = vm.heap.allocate(HeapObject::Str("orphan2".to_string()));

        vm.run_gc();

        assert_eq!(vm.heap.count(), 0);
    }

    #[test]
    fn test_gc_unreachable_cycle_collected() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        vm.gc = Gc::new(0);

        let a = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let b = vm.heap.allocate(HeapObject::Str("b".to_string()));
        *vm.heap.get_mut(&a).unwrap() = HeapObject::list(vec![b.clone()]);
        *vm.heap.get_mut(&b).unwrap() = HeapObject::list(vec![a.clone()]);

        vm.run_gc();

        assert!(!vm.heap.is_valid(&a));
        assert!(!vm.heap.is_valid(&b));
    }

    #[test]
    fn test_gc_reachable_cycle_survives_auto() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        vm.gc = Gc::new(0);

        let a = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let b = vm.heap.allocate(HeapObject::Str("b".to_string()));
        *vm.heap.get_mut(&a).unwrap() = HeapObject::list(vec![b.clone()]);
        *vm.heap.get_mut(&b).unwrap() = HeapObject::list(vec![a.clone()]);

        vm.add_live_ref(a.clone());
        vm.run_gc();

        assert!(vm.heap.is_valid(&a));
        assert!(vm.heap.is_valid(&b));
    }

    #[test]
    fn test_gc_multiple_roots_safe() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        vm.gc = Gc::new(0);

        let r1 = vm.heap.allocate(HeapObject::Str("root1".to_string()));
        let r2 = vm.heap.allocate(HeapObject::Str("root2".to_string()));
        let r3 = vm.heap.allocate(HeapObject::Str("root3".to_string()));
        let orphan = vm.heap.allocate(HeapObject::Str("orphan".to_string()));

        vm.add_live_ref(r1.clone());
        vm.add_live_ref(r2.clone());
        vm.add_live_ref(r3.clone());

        vm.run_gc();

        assert!(vm.heap.is_valid(&r1));
        assert!(vm.heap.is_valid(&r2));
        assert!(vm.heap.is_valid(&r3));
        assert!(!vm.heap.is_valid(&orphan));
    }

    #[test]
    fn test_gc_during_vm_execution_safe() {
        let source = "let A = [1, 2, 3]\nlet B = { x: 10, y: 20 }\nprint(A[0])\nprint(B.x)";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["1", "10"]);
    }

    #[test]
    fn test_gc_invalid_ref_after_collect() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        vm.gc = Gc::new(0);

        let r1 = vm.heap.allocate(HeapObject::Str("temp".to_string()));
        vm.run_gc();

        assert!(!vm.heap.is_valid(&r1));
        assert!(vm.heap.get(&r1).is_err());
    }

    #[test]
    fn test_gc_manual_debug_available() {
        let mut vm = ZeroVM::new(BytecodeProgram { header: BytecodeHeader::current(), entry_point: 0, constants: vec![], instructions: vec![], functions: vec![], function_bodies: vec![], default_bodies: vec![] });
        vm.gc = Gc::new(0);

        let _r1 = vm.heap.allocate(HeapObject::Str("a".to_string()));
        let _r2 = vm.heap.allocate(HeapObject::Str("b".to_string()));

        vm.run_gc();

        assert_eq!(vm.gc().stats().collections, 1);
        assert_eq!(vm.gc().stats().objects_freed, 2);
    }

    #[test]
    fn test_gc_regression_original_tests() {
        let output = run_source("print(10 + 20)").unwrap();
        assert_eq!(output, vec!["30"]);
    }

    // ===== Job 020: GC root coverage per §121/§131 =====
    //
    // Threshold 0 = collect at every instruction boundary, so every test
    // below is a survival check for one root source of §121.

    /// §121 roots "local variables pada Execution Frame yang aktif" and
    /// "parameter fungsi": frame locals and parameters survive collection.
    #[test]
    fn test_job020_params_and_frame_locals_survive_gc() {
        let (out, collections, _) = run_source_with_gc_pressure(
            "function compute(a, b) {\n    let locals = [a, b]\n    let scratch = [1, 2, 3]\n    let more = [4, 5, 6]\n    return locals[0] + locals[1]\n}\nprint(compute(7, 8))",
            0,
        )
        .unwrap();
        assert_eq!(out, vec!["15"]);
        assert!(collections > 0, "GC must actually run during execution");
    }

    /// §121 root "nilai pada Value Stack": the callee waits on the stack
    /// while the argument expression allocates under collection pressure.
    #[test]
    fn test_job020_value_stack_survives_gc() {
        let (out, _, _) = run_source_with_gc_pressure(
            "function id(x) { return x }\nfunction make() { let t = [5, 6]\nreturn t }\nprint(id(make())[0])",
            0,
        )
        .unwrap();
        assert_eq!(out, vec!["5"]);
    }

    /// §121 root "global bindings".
    #[test]
    fn test_job020_global_bindings_survive_gc() {
        let (out, _, _) = run_source_with_gc_pressure(
            "let config = [1, 2, 3]\nlet junk1 = [9, 9]\nlet junk2 = [8, 8]\nprint(config[2])",
            0,
        )
        .unwrap();
        assert_eq!(out, vec!["3"]);
    }

    /// §121 roots: an outer scope binding stays alive while a nested
    /// block allocates, and again after the block scope is popped.
    #[test]
    fn test_job020_scope_stack_survives_gc() {
        let (out, _, _) = run_source_with_gc_pressure(
            "let outer = [7]\nif true {\n    let inner = [1, 2]\n    let other = [3, 4]\n    print(inner[1])\n}\nprint(outer[0])",
            0,
        )
        .unwrap();
        assert_eq!(out, vec!["2", "7"]);
    }

    /// §131 "closure environment": the captured env outlives the frame
    /// that created it, under collection pressure.
    #[test]
    fn test_job020_closure_env_survives_gc() {
        let (out, _, _) = run_source_with_gc_pressure(
            "function make() {\n    let n = 41\n    return function() { return n + 1 }\n}\nlet c = make()\nlet junk = [1, 2, 3]\nprint(c())",
            0,
        )
        .unwrap();
        assert_eq!(out, vec!["42"]);
    }

    /// §121 root "task yang masih berjalan": a suspended task's saved
    /// context (frame locals) survives while other code allocates.
    #[test]
    fn test_job020_suspended_task_context_survives_gc() {
        let (out, _, _) = run_source_with_gc_pressure(
            "async function worker() {\n    let mine = [42]\n    await task.sleep(1)\n    print(mine[0])\n}\nasync function main() {\n    let t = worker()\n    let junk = [1, 2]\n    await t\n}\nmain()",
            0,
        )
        .unwrap();
        assert_eq!(out, vec!["42"]);
    }

    /// §107/§121: a completed task's stored heap result stays valid until
    /// awaited, across a collection storm in between.
    #[test]
    fn test_job020_completed_task_result_survives_gc() {
        let (out, _, _) = run_source_with_gc_pressure(
            "async function quick() {\n    let v = [77, 88]\n    return v\n}\nasync function main() {\n    let t = quick()\n    await task.sleep(1)\n    let junk = [1, 2, 3]\n    print((await t)[1])\n}\nmain()",
            0,
        )
        .unwrap();
        assert_eq!(out, vec!["88"]);
    }

    /// §131 root updates: a binding stops being a root the moment its
    /// scope is popped — "runtime harus memperbarui reference ... ketika
    /// binding ... dihentikan".
    #[test]
    fn test_job020_popped_scope_binding_is_reclaimable() {
        let mut vm = ZeroVM::new(BytecodeProgram {
            header: BytecodeHeader::current(),
            entry_point: 0,
            constants: vec![],
            instructions: vec![],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        });
        let r = vm.heap.allocate(HeapObject::list(vec![]));
        let mut scope = HashMap::new();
        scope.insert(
            "x".to_string(),
            Binding::new(Value::List(r.clone()), false, true),
        );
        vm.call_stack.push(Frame {
            function_index: 0,
            scopes: vec![scope],
            stack: Vec::new(),
            return_pc: 0,
            captured_env: None,
            scope_env: None,
            return_instructions: None,
            task_id: None,
        });

        vm.run_gc();
        assert!(vm.heap.is_valid(&r), "binding in scope must stay rooted");

        vm.call_stack.last_mut().unwrap().scopes.pop();
        vm.run_gc();
        assert!(!vm.heap.is_valid(&r), "popped scope must release its root");
    }

    /// §121: unreachable temporaries created during real execution are
    /// actually reclaimed while the program keeps running.
    #[test]
    fn test_job020_unreachable_temps_reclaimed_during_execution() {
        let (out, collections, heap_count) = run_source_with_gc_pressure(
            "let keep = [1, 2, 3]\nlet i = 0\nwhile i < 50 {\n    let t = [i, i]\n    i = i + 1\n}\nprint(keep[0])",
            8,
        )
        .unwrap();
        assert_eq!(out, vec!["1"]);
        assert!(
            collections >= 5,
            "expected repeated collections, got {}",
            collections
        );
        // keep list + its 3 elements + at most one threshold-window of
        // uncollected temporaries. Without reclamation this would be 150+.
        assert!(
            heap_count <= 16,
            "heap must not retain loop temporaries, count={}",
            heap_count
        );
    }

    /// The catchable-error path builds its error object mid-instruction;
    /// it must survive the boundary collections before the handler reads
    /// e.message.
    #[test]
    fn test_job020_error_object_survives_gc_pressure() {
        let (out, _, _) = run_source_with_gc_pressure(
            "try {\n    let l = [1, 2]\n    let x = l[9]\n    print(\"no error\")\n} catch e {\n    print(\"caught\")\n    print(e.message)\n}",
            0,
        )
        .unwrap();
        assert_eq!(out.len(), 2, "{:?}", out);
        assert_eq!(out[0], "caught");
        assert!(out[1].contains("out of bounds"), "{}", out[1]);
    }

    /// Native string operations allocate heap strings between boundaries
    /// (§145 native-call boundary under GC pressure).
    #[test]
    fn test_job020_native_string_path_survives_gc_pressure() {
        let (out, collections, _) = run_source_with_gc_pressure(
            "let a = \"hello \"\nlet b = a + \"world\"\nlet c = b + \"!\"\nprint(c)",
            0,
        )
        .unwrap();
        assert_eq!(out, vec!["hello world!"]);
        assert!(collections > 0);
    }

    /// §121 "module bindings yang masih aktif": an exported list held in
    /// run_modules' Rust-side export cache (a genuine runtime pin) and in
    /// the importing module's scope survives collection during module
    /// evaluation.
    #[test]
    fn test_job020_module_bindings_survive_gc_pressure() {
        fn parse_program(source: &str) -> crate::ast::Program {
            let mut lexer = Lexer::new(source);
            let tokens = lexer.tokenize().unwrap();
            let mut parser = Parser::new(tokens);
            parser.parse().unwrap()
        }

        let a = crate::compiler::compile_module(
            "./a.zs",
            &parse_program("let shared = [1, 2, 3]\nexport { shared }"),
        )
        .unwrap();
        let a_len = a.instructions.len();
        let b = crate::compiler::compile_module(
            "./b.zs",
            &parse_program(
                "import { shared } from \"./a\"\nlet junk = [9, 9]\nlet junk2 = [8, 8]\nprint(shared[1])",
            ),
        )
        .unwrap();
        let b_len = b.instructions.len();

        let merged = crate::compiler::merge_modules(&[a, b]);
        let modules = vec![
            crate::module::ModuleMeta {
                path: "./a.zs".to_string(),
                start: 0,
                end: a_len,
                exports: vec!["shared".to_string()],
                imports: vec![],
                re_exports: vec![],
            },
            crate::module::ModuleMeta {
                path: "./b.zs".to_string(),
                start: a_len,
                end: a_len + b_len,
                exports: vec![],
                imports: vec![(
                    "shared".to_string(),
                    "./a.zs".to_string(),
                    "shared".to_string(),
                )],
                re_exports: vec![],
            },
        ];

        let mut vm = ZeroVM::new(merged);
        vm.gc = Gc::new(0);
        vm.run_modules(&modules).unwrap();
        assert_eq!(vm.get_output(), &["2"]);
    }

    // ===== Job 021: closure in data structures shares its environment
    // (ZeroVM.md §130, Language.md §31/§35) =====

    /// Storage into a list keeps env identity: a mutation through the
    /// boxed copy is visible to the original closure, and a mutation
    /// through the original is visible to the boxed copy.
    #[test]
    fn test_job021_closure_in_list_shares_env() {
        let source = "function make() {\n    let n = 0\n    function bump() { n += 1\n    return n }\n    let holder = [bump]\n    print(holder[0]())\n    print(bump())\n    print(holder[0]())\n}\nmake()";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["1", "2", "3"]);
    }

    /// Object-literal storage path (execute_make_object).
    #[test]
    fn test_job021_closure_in_object_literal_shares_env() {
        let source = "function make() {\n    let n = 0\n    function bump() { n += 1\n    return n }\n    let obj = { f: bump }\n    print(obj.f())\n    print(bump())\n}\nmake()";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["1", "2"]);
    }

    /// set_property storage path (obj.f = bump).
    #[test]
    fn test_job021_closure_set_property_shares_env() {
        let source = "function make() {\n    let n = 0\n    function bump() { n += 1\n    return n }\n    let obj = {}\n    obj.f = bump\n    print(obj.f())\n    print(bump())\n}\nmake()";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["1", "2"]);
    }

    /// list.add storage path.
    #[test]
    fn test_job021_closure_list_add_shares_env() {
        let source = "function make() {\n    let n = 0\n    function bump() { n += 1\n    return n }\n    let items = []\n    list.add(items, bump)\n    print(items[0]())\n    print(bump())\n}\nmake()";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["1", "2"]);
    }

    /// store_in_env path: a closure assigned to a captured variable is
    /// read back through the heap environment by a sibling closure, and
    /// its mutations reach the original closure (§130 shared environment).
    #[test]
    fn test_job021_closure_stored_into_env_shares() {
        let source = "function make() {\n    let n = 0\n    function bump() { n += 1\n    return n }\n    let pick = null\n    function setPick() { pick = bump }\n    function getP() { return pick() }\n    setPick()\n    print(getP())\n    print(bump())\n}\nmake()";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["1", "2"]);
    }

    /// execute_make_closure populate/new-env path: a captured variable
    /// holding a closure is boxed into the new environment — the boxed
    /// copy must keep the original env (closure captures closure).
    #[test]
    fn test_job021_closure_captured_in_new_env_shares() {
        let source = "function make() {\n    let n = 0\n    function bump() { n += 1\n    return n }\n    function wrapper() {\n        function twice() { bump()\n            bump() }\n        return twice\n    }\n    let w = wrapper()\n    w()\n    print(bump())\n}\nmake()";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["3"]);
    }

    /// Job 020 Scope D verification: a closure boxed inside a list
    /// survives threshold-0 collections (child_refs through List →
    /// Closure → env) and still shares its environment afterwards.
    #[test]
    fn test_job021_closure_in_list_survives_gc_pressure() {
        let (out, collections, _) = run_source_with_gc_pressure(
            "function make() {\n    let n = 0\n    function bump() { n += 1\n    return n }\n    let holder = [bump]\n    let i = 0\n    while i < 5 {\n        let junk = [i, i + 1]\n        i = i + 1\n    }\n    print(holder[0]())\n    print(bump())\n    print(holder[0]())\n}\nmake()",
            0,
        )
        .unwrap();
        assert_eq!(out, vec!["1", "2", "3"]);
        assert!(collections > 0, "expected collections under threshold 0");
    }

    /// Scope E3 (§130): sibling closures from the same scope keep
    /// observing each other after one of them is stored in a list.
    #[test]
    fn test_job021_siblings_share_via_stored_closure() {
        let source = "function make() {\n    let n = 0\n    function bump() { n += 1\n    return n }\n    function read() { return n }\n    let holder = [bump]\n    holder[0]()\n    print(read())\n    bump()\n    print(holder[0]())\n}\nmake()";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["1", "3"]);
    }

    /// Scope C round-trip: Value → RuntimeValue → Value preserves env
    /// identity and allocates only the Closure (no cloned Environment).
    #[test]
    fn test_job021_to_runtime_value_roundtrip_preserves_env() {
        let bc = compile_source("function noop() { return 1 }\nnoop()");
        let mut vm = ZeroVM::new(bc);
        let num = vm.heap.allocate(HeapObject::Number(7.0));
        let env = vm.heap.allocate(HeapObject::Environment(vec![EnvBinding {
            name: "n".to_string(),
            value: num,
            is_const: false,
        }]));
        let closure = Value::Closure {
            function_index: 0,
            env: env.clone(),
            param_defaults: vec![],
        };
        let before = vm.heap.count();
        let rv = closure.to_runtime_value(&mut vm.heap);
        assert_eq!(
            vm.heap.count(),
            before + 1,
            "boxing must allocate only the Closure, not a cloned Environment"
        );
        match Value::from_runtime_value(&rv, &vm.heap).unwrap() {
            Value::Closure { env: env_back, .. } => assert_eq!(env_back, env),
            other => panic!("expected closure round-trip, got {:?}", other),
        }
    }

    // ===== Job 022: default parameters evaluated at call time (§30) =====

    /// §30 "Default dievaluasi saat function dipanggil": the default reads
    /// the outer variable's value at the call, not at compile time.
    #[test]
    fn test_job022_default_evaluated_at_call_time() {
        let output =
            run_source("let x = 1\nfunction f(a = x) { return a }\nx = 2\nprint(f())").unwrap();
        assert_eq!(output, vec!["2"]);
    }

    /// §30: each call re-evaluates the default — a mutation made through the
    /// default itself is visible on the next call.
    #[test]
    fn test_job022_default_sees_outer_mutation_each_call() {
        // The counter lives in outer's scope and is captured by both inc
        // and g — a shared env (§66/§130) so the default's own mutation is
        // visible to the next call's evaluation.
        let output = run_source(
            "function outer() {\n    let counter = 0\n    function inc() { counter += 1\n        return counter }\n    function g(n = inc()) { return n }\n    print(g())\n    print(g())\n}\nouter()",
        )
        .unwrap();
        assert_eq!(output, vec!["1", "2"]);
    }

    /// §30: a default may reference the previous parameter.
    #[test]
    fn test_job022_default_uses_previous_parameter() {
        let output = run_source("function h(a, b = a + 1) { return b }\nprint(h(5))").unwrap();
        assert_eq!(output, vec!["6"]);
    }

    /// §30: explicit value overrides the default (the default body does not
    /// run when the argument is present).
    #[test]
    fn test_job022_explicit_argument_overrides_default() {
        let output = run_source("function f(a = 99) { return a }\nprint(f(1))\nprint(f())").unwrap();
        assert_eq!(output, vec!["1", "99"]);
    }

    /// §30: a missing argument without a default becomes null.
    #[test]
    fn test_job022_missing_argument_without_default_is_null() {
        let output = run_source("function f(a) { return a }\nprint(f())").unwrap();
        assert_eq!(output, vec!["null"]);
    }

    /// §30/§31: a closure's default reads its captured environment at call
    /// time (the live value), and a default resolving a global sees the
    /// global's current value. The job file's literal sample (top-level
    /// `x = 30` against a shadowed local `x`) yields 20/20 under §31
    /// lexical scoping — recorded in ISSUES.md §70a.
    #[test]
    fn test_job022_closure_call_default_uses_captured_env() {
        // Captured local: mutated inside outer before the return, the call
        // must observe the live value from the shared environment (§130).
        let output = run_source(
            "function outer() {\n    let x = 20\n    let f = function(a = x) { return a }\n    x = 99\n    return f\n}\nprint(outer()())",
        )
        .unwrap();
        assert_eq!(output, vec!["99"]);
        // A real closure (y forces the capture) whose default also reads a
        // global: the call-time value of the global flows in.
        let output = run_source(
            "let x = 10\nfunction make() {\n    let y = 5\n    return function(a = x + y) { return a }\n}\nlet f = make()\nprint(f())\nx = 30\nprint(f())",
        )
        .unwrap();
        assert_eq!(output, vec!["15", "35"]);
    }

    /// §101 + Job 022 AC: calling an async function only creates the task —
    /// the default runs when the scheduler starts it (prepare_task_start),
    /// before the body; the async closure path passes the task env.
    #[test]
    fn test_job022_async_default_evaluated_at_task_start() {
        let output = run_source(
            "let counts = {calls: 0}\nfunction tick() { counts.calls += 1\n    return 42 }\nasync function af(a = tick()) { return a }\nfunction mk() {\n    let y = 41\n    async function inner(a = y + 1) { return a }\n    return inner\n}\nasync function main() {\n    let t = af()\n    print(counts.calls)\n    print(await t)\n    print(await mk()())\n    print(counts.calls)\n}\nmain()",
        )
        .unwrap();
        assert_eq!(output, vec!["0", "42", "42", "1"]);
    }

    /// §E/§121: default evaluation under threshold-0 GC — the list allocated
    /// by a default body and the captured list/string a default reads stay
    /// rooted through the evaluation frame.
    #[test]
    fn test_job022_default_heap_alloc_survives_gc() {
        let (out, collections, _) = run_source_with_gc_pressure(
            "function make() {\n    let base = [\"kept\", 2]\n    return function(tag = base[0] + \"!\") { return tag }\n}\nfunction g(a = [7, 8, 9]) { return a[0] }\nprint(g())\nlet f = make()\nprint(f())",
            0,
        )
        .unwrap();
        assert_eq!(out, vec!["7", "kept!"]);
        assert!(collections > 0, "GC must actually run during execution");
    }

    /// §30: chained defaults evaluate left to right; explicit arguments feed
    /// the chain.
    #[test]
    fn test_job022_nested_defaults_chain() {
        let output = run_source(
            "function f(a = 1, b = a + 2, c = b + 3) { return c }\nprint(f())\nprint(f(10))\nprint(f(10, 20))",
        )
        .unwrap();
        assert_eq!(output, vec!["6", "15", "23"]);
    }

    /// §30 across declaration forms: arrow function defaults (site 3), and
    /// arrow parameters bound as locals — they were never declared before
    /// Job 022, which defaults cannot work without.
    #[test]
    fn test_job022_arrow_function_default_params() {
        let output =
            run_source("let f = (d = 7) => d\nprint(f())\nprint(f(9))\nlet g = (a) => a + 1\nprint(g(5))")
                .unwrap();
        assert_eq!(output, vec!["7", "9", "6"]);
    }

    /// Job 022 AC: the default representation survives every hop —
    /// FunctionInfo → execute_make_closure → Value::Closure → list storage
    /// (heap_object_for_element) → from_heap_ref → call, so the retrieved
    /// closure still evaluates its default.
    #[test]
    fn test_job022_closure_param_defaults_roundtrip_through_heap() {
        let output = run_source(
            "function outer() {\n    let tag = \"heap\"\n    let f = function(a = tag + \"!\") { return a }\n    let box = [f]\n    let g = box[0]\n    return g()\n}\nprint(outer())",
        )
        .unwrap();
        assert_eq!(output, vec!["heap!"]);
    }

    // ===== Closure tests =====

    #[test]
    fn test_closure_basic_invocation() {
        let source = "function outer() {\n    return function() { return 42 }\n}\nlet f = outer()\nprint(f())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn test_closure_capture_variable() {
        let source = "function outer() {\n    let x = 10\n    return function() { return x }\n}\nlet f = outer()\nprint(f())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_closure_mutation_shared() {
        let source = "function outer() {\n    let count = 0\n    function increment() { count += 1 }\n    function read() { return count }\n    increment()\n    increment()\n    return read\n}\nlet f = outer()\nprint(f())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["2"]);
    }

    #[test]
    fn test_closure_multiple_closures_same_env() {
        let source = "function outer() {\n    let x = 0\n    function a() { x += 10 }\n    function b() { return x }\n    a()\n    return b\n}\nlet f = outer()\nprint(f())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_closure_survives_outer() {
        let source = "function make_adder(n) {\n    return function(x) { return x + n }\n}\nlet add5 = make_adder(5)\nprint(add5(3))";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["8"]);
    }

    #[test]
    fn test_closure_nested() {
        let source = "function outer() {\n    let x = 1\n    function middle() {\n        let y = 2\n        function inner() { return x + y }\n        return inner\n    }\n    return middle()\n}\nlet f = outer()\nprint(f())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["3"]);
    }

    #[test]
    fn test_closure_recursion() {
        let source = "function factorial(n) {\n    if n <= 1 { return 1 }\n    return n * factorial(n - 1)\n}\nprint(factorial(5))";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["120"]);
    }

    #[test]
    fn test_closure_with_default_params() {
        let source = "function make_greeting(greeting = \"Hello\") {\n    return function(name) { return greeting + \" \" + name }\n}\nlet greet = make_greeting()\nprint(greet(\"World\"))";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["Hello World"]);
    }

    #[test]
    fn test_closure_default_param_uses_outer() {
        let source = "function outer() {\n    let prefix = \"Hi\"\n    function make(prefix = prefix) {\n        return function(name) { return prefix + \" \" + name }\n    }\n    return make()\n}\nlet f = outer()\nprint(f(\"There\"))";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["Hi There"]);
    }

    #[test]
    fn test_closure_arrow_function() {
        let source = "function outer() {\n    let x = 10\n    return () => x\n}\nlet f = outer()\nprint(f())";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_closure_print_function() {
        let output = run_source("print(\"hello\")").unwrap();
        assert_eq!(output, vec!["hello"]);
    }

    #[test]
    fn test_closure_debug_bytecode() {
        let source = "function outer() {\n    let x = 10\n    return function() { return x }\n}\nlet f = outer()\nprint(f())";
        let bc = compile_source(source);
        
        println!("\n=== Instructions ===");
        for (i, instr) in bc.instructions.iter().enumerate() {
            println!("  {}: {:?}", i, instr);
        }
        
        println!("\n=== Functions ===");
        for (i, func) in bc.functions.iter().enumerate() {
            println!("  {}: {:?}", i, func);
        }
        
        println!("\n=== Constants ===");
        for (i, c) in bc.constants.iter().enumerate() {
            println!("  {}: {:?}", i, c);
        }
    }

    #[test]
    fn test_arrow_debug_trace() {
        let source = "function outer() {\n    let x = 10\n    return () => x\n}\nlet f = outer()\nprint(f())";
        let bc = compile_source(source);

        eprintln!("\n=== Constants ===");
        for (i, c) in bc.constants.iter().enumerate() {
            eprintln!("  {}: {:?}", i, c);
        }
        eprintln!("\n=== Functions ===");
        for (i, f) in bc.functions.iter().enumerate() {
            eprintln!("  {}: {:?}", i, f);
        }
        eprintln!("\n=== Function Bodies ===");
        for (i, body) in bc.function_bodies.iter().enumerate() {
            eprintln!("  Body {}:", i);
            for (j, instr) in body.iter().enumerate() {
                eprintln!("    {}: {:?}", j, instr);
            }
        }
        eprintln!("\n=== Main Instructions (first 12) ===");
        for (i, instr) in bc.instructions.iter().enumerate().take(12) {
            eprintln!("  {}: {:?}", i, instr);
        }

        match run_source(source) {
            Ok(output) => eprintln!("Output: {:?}", output),
            Err(e) => eprintln!("Error: {:?}", e),
        }
    }

    fn compile_try(source: &str) -> Result<BytecodeProgram, crate::error::CompilerError> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        let program = parser.parse().unwrap();
        compile(&program)
    }

    fn parse_try(source: &str) -> Result<crate::ast::Program, crate::error::ParserError> {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        parser.parse()
    }

    fn run_vm(source: &str) -> Result<ZeroVM, VmError> {
        let bc = compile_source(source);
        let mut vm = ZeroVM::new(bc);
        vm.run()?;
        Ok(vm)
    }

    #[test]
    fn test_job009_try_catch_index_out_of_bounds() {
        let output = run_source(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print(\"caught\")\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["caught", "after"]);
    }

    #[test]
    fn test_job009_catch_e_contains_error_message() {
        let vm = run_vm(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print.err(e)\n}",
        )
        .unwrap();
        let err = vm.get_error_output();
        assert_eq!(err.len(), 1);
        assert!(err[0].contains("out of bounds"), "{}", err[0]);
    }

    #[test]
    fn test_job009_finally_runs_on_try_success() {
        let output = run_source(
            "try {\n    print(\"try\")\n} catch e {\n    print(\"catch\")\n} finally {\n    print(\"finally\")\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["try", "finally", "after"]);
    }

    #[test]
    fn test_job009_finally_runs_when_catch_handles_error() {
        let output = run_source(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print(\"catch\")\n} finally {\n    print(\"finally\")\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["catch", "finally", "after"]);
    }

    #[test]
    fn test_job009_finally_runs_when_no_catch_then_rethrows() {
        let err = run_source(
            "try {\n    let A = [10]\n    print(A[99])\n} finally {\n    print(\"finally\")\n}",
        )
        .unwrap_err();
        assert!(matches!(err, VmError::RuntimeError(_)), "{}", err);
    }

    #[test]
    fn test_job009_nested_try_catch() {
        let output = run_source(
            "try {\n    try {\n        let A = [10]\n        print(A[99])\n    } catch inner {\n        print(\"inner\")\n    }\n} catch outer {\n    print(\"outer\")\n}\nprint(\"done\")",
        )
        .unwrap();
        assert_eq!(output, vec!["inner", "done"]);
    }

    #[test]
    fn test_job009_error_outside_try_still_fails() {
        let err = run_source("let A = [10]\nprint(A[99])").unwrap_err();
        assert!(matches!(err, VmError::RuntimeError(_)), "{}", err);
    }

    #[test]
    fn test_job009_print_err_displays_error() {
        let vm = run_vm("try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print.err(e)\n}").unwrap();
        assert_eq!(vm.get_error_output().len(), 1);
        assert!(vm.get_error_output()[0].contains("out of bounds"));
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job009_type_number() {
        let output = run_source("print(type(10))").unwrap();
        assert_eq!(output, vec!["number"]);
    }

    #[test]
    fn test_job009_type_string() {
        let output = run_source(r#"print(type("Hello"))"#).unwrap();
        assert_eq!(output, vec!["string"]);
    }

    #[test]
    fn test_job009_type_boolean() {
        let output = run_source("print(type(true))").unwrap();
        assert_eq!(output, vec!["boolean"]);
    }

    #[test]
    fn test_job009_type_null() {
        let output = run_source("print(type(null))").unwrap();
        assert_eq!(output, vec!["null"]);
    }

    #[test]
    fn test_job009_type_list() {
        let output = run_source("print(type([1, 2]))").unwrap();
        assert_eq!(output, vec!["list"]);
    }

    #[test]
    fn test_job009_type_object() {
        let output = run_source("print(type({}))").unwrap();
        assert_eq!(output, vec!["object"]);
    }

    #[test]
    fn test_job009_type_function() {
        let output = run_source("function test() {}\nprint(type(test))").unwrap();
        assert_eq!(output, vec!["function"]);
    }

    #[test]
    fn test_job009_type_returns_string() {
        let output = run_source("print(type(type(10)))").unwrap();
        assert_eq!(output, vec!["string"]);
    }

    #[test]
    fn test_job009_number_from_string_success() {
        let output = run_source("print(number(\"123\"))").unwrap();
        assert_eq!(output, vec!["123"]);
    }

    #[test]
    fn test_job009_number_from_decimal_string_success() {
        let output = run_source("print(number(\"3.14\"))").unwrap();
        assert_eq!(output, vec!["3.14"]);
    }

    #[test]
    fn test_job009_number_invalid_string_error() {
        let err = run_source("print(number(\"hello\"))").unwrap_err();
        assert!(matches!(err, VmError::RuntimeError(_)), "{}", err);
    }

    #[test]
    fn test_job009_string_from_number_success() {
        let output = run_source("print(string(123))").unwrap();
        assert_eq!(output, vec!["123"]);
    }

    #[test]
    fn test_job009_string_from_boolean_success() {
        let output = run_source("print(string(true))").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_job009_string_from_list_error() {
        let err = run_source("print(string([1, 2]))").unwrap_err();
        assert!(matches!(err, VmError::RuntimeError(_)), "{}", err);
    }

    #[test]
    fn test_job009_boolean_identity_success() {
        let output = run_source("print(boolean(true))").unwrap();
        assert_eq!(output, vec!["true"]);
        let output = run_source("print(boolean(false))").unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_job009_boolean_from_number_error() {
        let err = run_source("print(boolean(1))").unwrap_err();
        assert!(matches!(err, VmError::RuntimeError(_)), "{}", err);
    }

    #[test]
    fn test_job009_boolean_from_string_error() {
        let err = run_source("print(boolean(\"true\"))").unwrap_err();
        assert!(matches!(err, VmError::RuntimeError(_)), "{}", err);
    }

    #[test]
    fn test_job009_conversion_errors_catchable() {
        let output = run_source(
            "try {\n    print(number(\"hello\"))\n} catch e {\n    print(\"caught-conv\")\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["caught-conv", "after"]);
    }

    #[test]
    fn test_job009_try_catch_exact_example_section43() {
        let vm = run_vm(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print.err(e)\n}",
        )
        .unwrap();
        assert_eq!(vm.get_error_output().len(), 1);
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job009_return_from_try_runs_finally() {
        let output = run_source(
            "function f() {\n    try {\n        return 1\n    } finally {\n        print(\"fin\")\n    }\n}\nprint(f())",
        )
        .unwrap();
        assert_eq!(output, vec!["fin", "1"]);
    }

    #[test]
    fn test_job009_error_in_function_caught_by_caller_try() {
        let output = run_source(
            "function boom() {\n    let A = [10]\n    return A[99]\n}\ntry {\n    print(boom())\n} catch e {\n    print(\"caught-fn\")\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["caught-fn", "after"]);
    }

    #[test]
    fn test_job009_parse_error_not_caught() {
        // Parse/syntax errors occur before execution and cannot be caught (§43)
        let tokens = Lexer::new("try { let = 1 } catch e { }").tokenize();
        let parse_result = match tokens {
            Ok(toks) => {
                let mut p = Parser::new(toks);
                p.parse().map(|_| ())
            }
            Err(e) => Err(crate::error::ParserError::new(
                &e.message,
                e.line,
                e.column,
            )),
        };
        assert!(parse_result.is_err());
    }

    #[test]
    fn test_job008_list_assignment_shares_reference() {
        let output = run_source("let A = [10, 20]\nlet B = A\nB[0] = 99\nprint(A[0])").unwrap();
        assert_eq!(output, vec!["99"]);
    }

    #[test]
    fn test_job008_object_assignment_shares_reference() {
        let output =
            run_source("let user = { name: \"Kael\" }\nlet u2 = user\nu2.name = \"Budi\"\nprint(user.name)")
                .unwrap();
        assert_eq!(output, vec!["Budi"]);
    }

    #[test]
    fn test_job008_list_passed_to_function_mutation_visible() {
        let source = "function mutate(L) {\n    L[0] = 99\n}\nlet A = [1, 2]\nmutate(A)\nprint(A[0])";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["99"]);
    }

    #[test]
    fn test_job008_object_passed_to_function_mutation_visible() {
        let source =
            "function mutateO(O) {\n    O.x = 99\n}\nlet o = { x: 1 }\nmutateO(o)\nprint(o.x)";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["99"]);
    }

    #[test]
    fn test_job008_function_returning_new_list_independent() {
        let source =
            "function make() { return [1, 2] }\nlet A = make()\nlet B = make()\nA[0] = 99\nprint(A[0])\nprint(B[0])";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["99", "1"]);
    }

    #[test]
    fn test_job008_function_returning_new_object_independent() {
        let source =
            "function make() { return { v: 1 } }\nlet A = make()\nlet B = make()\nA.v = 99\nprint(A.v)\nprint(B.v)";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["99", "1"]);
    }

    #[test]
    fn test_job008_set_index_mutates_heap() {
        let output = run_source("let A = [1, 2, 3]\nA[1] = 99\nprint(A[1])").unwrap();
        assert_eq!(output, vec!["99"]);
    }

    #[test]
    fn test_job008_set_property_mutates_heap_existing_key() {
        let output = run_source("let user = { name: \"Kael\" }\nuser.name = \"Budi\"\nprint(user.name)")
            .unwrap();
        assert_eq!(output, vec!["Budi"]);
    }

    #[test]
    fn test_job008_set_property_mutates_heap_new_key() {
        let output = run_source("let user = { name: \"Kael\" }\nuser.age = 18\nprint(user.age)").unwrap();
        assert_eq!(output, vec!["18"]);
    }

    #[test]
    fn test_job008_const_reassignment_compile_error() {
        let err = compile_try("const A = 10\nA = 20").unwrap_err();
        assert!(err.message.contains("Cannot assign to const 'A'"), "{}", err.message);
    }

    #[test]
    fn test_job008_const_without_value_is_null() {
        let output = run_source("const A\nprint(A)").unwrap();
        assert_eq!(output, vec!["null"]);
    }

    #[test]
    fn test_job008_const_without_value_assignment_error() {
        let err = compile_try("const A\nA = 10").unwrap_err();
        assert!(err.message.contains("Cannot assign to const 'A'"), "{}", err.message);
    }

    #[test]
    fn test_job008_const_list_element_mutation_allowed() {
        // ISSUES §58b: const is a binding rule, not a heap-object rule.
        let output = run_source("const A = [10, 20]\nA[0] = 30\nprint(A)").unwrap();
        assert_eq!(output, vec!["[30, 20]"]);
    }

    #[test]
    fn test_job008_const_alias_does_not_freeze_object() {
        // ISSUES §58b: `const C = A` does not freeze A's object.
        let output =
            run_source("let A = [1, 2]\nconst C = A\nA[0] = 9\nC[1] = 8\nprint(A)").unwrap();
        assert_eq!(output, vec!["[9, 8]"]);
    }

    #[test]
    fn test_job008_const_returned_collection_stored_in_let_mutable() {
        // ISSUES §58b: a collection returned from a const binding stays mutable
        // once the result is stored in `let`.
        let output = run_source(
            "function make() {\n    const A = [1, 2]\n    return A\n}\nlet B = make()\nB[0] = 9\nprint(B)",
        )
        .unwrap();
        assert_eq!(output, vec!["[9, 2]"]);
    }

    #[test]
    fn test_job008_const_object_property_mutation_existing_key_allowed() {
        // ISSUES §58b: const does not freeze the object.
        let output =
            run_source("const user = { name: \"Kael\" }\nuser.name = \"Budi\"\nprint(user.name)")
                .unwrap();
        assert_eq!(output, vec!["Budi"]);
    }

    #[test]
    fn test_job008_const_object_property_mutation_new_key_allowed() {
        let output =
            run_source("const user = { name: \"Kael\" }\nuser.age = 18\nprint(user.age)").unwrap();
        assert_eq!(output, vec!["18"]);
    }

    #[test]
    fn test_job008_captured_const_mutation_error() {
        let source = "function outer() {\n    const x = 1\n    return function() { x = 2 }\n}\nouter()()";
        let err = compile_try(source).unwrap_err();
        assert!(err.message.contains("Cannot assign to const 'x'"), "{}", err.message);
    }

    #[test]
    fn test_job008_duplicate_let_let_error() {
        let err = compile_try("let A = 10\nlet A = 20").unwrap_err();
        assert!(err.message.contains("Duplicate declaration of 'A'"), "{}", err.message);
    }

    #[test]
    fn test_job008_duplicate_let_const_error() {
        let err = compile_try("let A = 10\nconst A = 20").unwrap_err();
        assert!(err.message.contains("Duplicate declaration of 'A'"), "{}", err.message);
    }

    #[test]
    fn test_job008_duplicate_const_const_error() {
        let err = compile_try("const A = 10\nconst A = 20").unwrap_err();
        assert!(err.message.contains("Duplicate declaration of 'A'"), "{}", err.message);
    }

    #[test]
    fn test_job008_duplicate_const_let_error() {
        let err = compile_try("const A = 10\nlet A = 20").unwrap_err();
        assert!(err.message.contains("Duplicate declaration of 'A'"), "{}", err.message);
    }

    #[test]
    fn test_job008_shadowing_different_scopes_valid() {
        let source = "let A = 10\nif true {\n    let A = 20\n    print(A)\n}\nprint(A)";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["20", "10"]);
    }

    #[test]
    fn test_job008_duplicate_object_literal_key_error() {
        let err = parse_try("{ a: 1, a: 2 }").unwrap_err();
        assert!(err.message.contains("Duplicate object key 'a'"), "{}", err.message);
    }

    #[test]
    fn test_job008_inner_block_variable_does_not_leak() {
        let err = run_source("if true {\n    let A = 10\n}\nprint(A)").unwrap_err();
        assert!(
            err.to_string().contains("Undefined variable 'A'"),
            "{}",
            err
        );
    }

    #[test]
    fn test_job008_outer_variable_readable_from_inner() {
        let source = "let A = 10\nif true {\n    print(A)\n}";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_job008_function_scope_assignment_does_not_change_outer() {
        let source = "let A = 10\nfunction test() {\n    A = 20\n    print(A)\n}\ntest()\nprint(A)";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["20", "10"]);
    }

    #[test]
    fn test_job008_function_reads_global_without_capture() {
        let source = "let A = 10\nfunction test() {\n    print(A)\n}\ntest()";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["10"]);
    }

    #[test]
    fn test_job008_else_if_chain_depth_gt_2() {
        let source = "if false {\n    print(1)\n} else if false {\n    print(2)\n} else if false {\n    print(3)\n} else if true {\n    print(4)\n} else {\n    print(5)\n}";
        let output = run_source(source).unwrap();
        assert_eq!(output, vec!["4"]);
    }

    #[test]
    fn test_job008_break_outside_loop_compile_error() {
        let err = compile_try("break").unwrap_err();
        assert!(err.message.contains("break outside of loop"), "{}", err.message);
    }

    #[test]
    fn test_job008_continue_outside_loop_compile_error() {
        let err = compile_try("continue").unwrap_err();
        assert!(err.message.contains("continue outside of loop"), "{}", err.message);
    }

    #[test]
    fn test_job008_break_does_not_cross_function_boundary() {
        let err = compile_try("while true {\n    function f() {\n        break\n    }\n}").unwrap_err();
        assert!(err.message.contains("break outside of loop"), "{}", err.message);
    }

    #[test]
    fn test_job008_continue_does_not_cross_function_boundary() {
        let err =
            compile_try("while true {\n    function f() {\n        continue\n    }\n}").unwrap_err();
        assert!(err.message.contains("continue outside of loop"), "{}", err.message);
    }

    #[test]
    fn test_job008_ampersand_and_alias() {
        let output = run_source("print(true && false)\nprint(true || false)\nprint(not true)").unwrap();
        assert_eq!(output, vec!["false", "true", "false"]);
    }

    #[test]
    fn test_job008_and_or_not_identical_to_keywords() {
        let a = run_source("print(true and false)").unwrap();
        let b = run_source("print(true && false)").unwrap();
        assert_eq!(a, b);

        let a = run_source("print(false or true)").unwrap();
        let b = run_source("print(false || true)").unwrap();
        assert_eq!(a, b);

        let a = run_source("print(!true)").unwrap();
        let b = run_source("print(not true)").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_job008_no_truthy_and_non_boolean() {
        let err = run_source("print(10 and true)").unwrap_err();
        assert!(err.to_string().contains("Conditional jump requires boolean"), "{}", err);
    }

    #[test]
    fn test_job008_no_truthy_or_non_boolean() {
        let err = run_source("print(\"hello\" || false)").unwrap_err();
        assert!(err.to_string().contains("Conditional jump requires boolean"), "{}", err);
    }

    #[test]
    fn test_job008_not_non_boolean_error() {
        let err = run_source("print(not 10)").unwrap_err();
        assert!(err.to_string().contains("Cannot apply !"), "{}", err);
    }

    #[test]
    fn test_job008_raw_eq_still_true_for_number_string() {
        let output = run_source("print(10 === \"10\")\nprint(10 == \"10\")").unwrap();
        assert_eq!(output, vec!["true", "false"]);
    }

    #[test]
    fn test_job008_shared_reference_via_raw_eq() {
        let output = run_source("let A = [1]\nlet B = A\nprint(A === B)").unwrap();
        assert_eq!(output, vec!["true"]);
    }

    // ---- ISSUES §59a: caught error `e` is an object ----

    #[test]
    fn test_job059a_catch_e_is_object() {
        let output = run_source(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print(type(e))\n}",
        )
        .unwrap();
        assert_eq!(output, vec!["object"]);
    }

    #[test]
    fn test_job059a_e_fields_type_message_location() {
        let output = run_source(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print(e.type)\n    print(e.message)\n    print(e.location)\n}",
        )
        .unwrap();
        assert_eq!(output.len(), 3, "{:?}", output);
        assert_eq!(output[0], "IndexError");
        assert!(output[1].contains("out of bounds"), "{}", output[1]);
        // No source position is tracked for runtime errors (§154) → null.
        assert_eq!(output[2], "null");
    }

    #[test]
    fn test_job059a_string_e_returns_message() {
        let output = run_source(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print(string(e))\n    print(e.message)\n}",
        )
        .unwrap();
        assert_eq!(output.len(), 2, "{:?}", output);
        assert_eq!(output[0], output[1]);
        assert!(output[0].contains("out of bounds"), "{}", output[0]);
    }

    #[test]
    fn test_job059a_print_err_e_stays_human_readable() {
        let vm = run_vm(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print.err(e)\n}",
        )
        .unwrap();
        let err = vm.get_error_output();
        assert_eq!(err.len(), 1);
        assert!(err[0].contains("out of bounds"), "{}", err[0]);
        assert!(
            !err[0].contains('{'),
            "print.err prints the message, not the object dump: {}",
            err[0]
        );
    }

    #[test]
    fn test_job059a_print_e_shows_object() {
        let output = run_source(
            "try {\n    let A = [10]\n    print(A[99])\n} catch e {\n    print(e)\n}",
        )
        .unwrap();
        assert_eq!(output.len(), 1);
        assert!(output[0].starts_with("{type: IndexError"), "{}", output[0]);
        assert!(output[0].contains("location: null"), "{}", output[0]);
    }

    #[test]
    fn test_job059a_json_parse_failure_is_syntax_error() {
        let output = run_source(
            "try {\n    json.parse(\"{invalid json\")\n} catch e {\n    print(e.type)\n    print(e.message)\n}",
        )
        .unwrap();
        assert_eq!(output[0], "SyntaxError", "{:?}", output);
        assert!(output[1].contains("Invalid JSON"), "{}", output[1]);
    }

    #[test]
    fn test_job059a_type_error_category() {
        let output = run_source("try {\n    print(1 + \"x\")\n} catch e {\n    print(e.type)\n}")
            .unwrap();
        assert_eq!(output, vec!["TypeError"]);
        let output =
            run_source("try {\n    print(number(\"hello\"))\n} catch e {\n    print(e.type)\n}")
                .unwrap();
        assert_eq!(output, vec!["TypeError"]);
    }

    #[test]
    fn test_job059a_division_by_zero_category() {
        let output = run_source("try {\n    print(10 / 0)\n} catch e {\n    print(e.type)\n}")
            .unwrap();
        assert_eq!(output, vec!["DivisionError"]);
    }

    #[test]
    fn test_job059a_key_error_category() {
        let output =
            run_source("try {\n    let O = { a: 1 }\n    print(O.b)\n} catch e {\n    print(e.type)\n}")
                .unwrap();
        assert_eq!(output, vec!["KeyError"]);
    }

    #[test]
    fn test_job059a_syntax_error_not_catchable() {
        // Parse/syntax errors happen before execution → uncatchable (§43).
        let err = parse_try("try {\n    let = 1\n} catch e {\n    print(\"caught\")\n}").unwrap_err();
        assert!(!err.message.is_empty());
    }

    #[test]
    fn test_job059a_string_plain_object_still_errors() {
        // §54/§59c: string() on a non-error object stays a conversion error.
        let err = run_source("print(string({ a: 1 }))").unwrap_err();
        assert!(
            err.to_string().contains("Invalid conversion to string"),
            "{}",
            err
        );
    }

    #[test]
    fn test_job059a_plain_object_with_error_fields_is_not_error() {
        // Regression: identity of an error object is internal (ISSUES §59a) —
        // a plain object that happens to carry type/message/location stays an
        // ordinary object for string() and print.err().
        let err = run_source(
            "print(string({ type: \"CustomError\", message: \"boom\", location: null }))",
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("Invalid conversion to string"),
            "{}",
            err
        );

        let vm = run_vm("print.err({ type: \"CustomError\", message: \"boom\", location: null })")
            .unwrap();
        let out = vm.get_error_output();
        assert_eq!(out.len(), 1, "{:?}", out);
        assert!(out[0].starts_with("{type: CustomError"), "{}", out[0]);
        assert!(out[0].contains("message: boom"), "{}", out[0]);
    }

    // ---- ISSUES §59d: finally on break/continue early exit ----

    #[test]
    fn test_job059d_break_runs_finally() {
        let output = run_source(
            "while true {\n    try {\n        print(\"body\")\n        break\n    } finally {\n        print(\"fin\")\n    }\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["body", "fin", "after"]);
    }

    #[test]
    fn test_job059d_continue_runs_finally() {
        let output = run_source(
            "let i = 0\nwhile i < 3 {\n    i += 1\n    try {\n        if i == 2 { continue }\n    } finally {\n        print(\"fin\")\n    }\n    print(string(i))\n}\nprint(\"done\")",
        )
        .unwrap();
        assert_eq!(output, vec!["fin", "1", "fin", "fin", "3", "done"]);
    }

    #[test]
    fn test_job059d_break_nested_finally_innermost_first() {
        let output = run_source(
            "while true {\n    try {\n        try {\n            print(\"body\")\n            break\n        } finally {\n            print(\"inner\")\n        }\n    } finally {\n        print(\"outer\")\n    }\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["body", "inner", "outer", "after"]);
    }

    #[test]
    fn test_job059d_continue_nested_finally_innermost_first() {
        let output = run_source(
            "let i = 0\nwhile i < 2 {\n    i += 1\n    try {\n        try {\n            continue\n        } finally {\n            print(\"in\")\n        }\n    } finally {\n        print(\"out\")\n    }\n}\nprint(\"done\")",
        )
        .unwrap();
        assert_eq!(output, vec!["in", "out", "in", "out", "done"]);
    }

    #[test]
    fn test_job059d_break_inside_finally_body() {
        // Break inside a finally body discards that finally's own marker and
        // leaves the loop; no other finally is crossed.
        let output = run_source(
            "while true {\n    try {\n        print(\"body\")\n    } finally {\n        print(\"fin\")\n        break\n    }\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["body", "fin", "after"]);
    }

    #[test]
    fn test_job059d_break_runs_finally_in_infinite_loop_stmt() {
        let output = run_source(
            "loop {\n    try {\n        break\n    } finally {\n        print(\"fin\")\n    }\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["fin", "after"]);
    }

    #[test]
    fn test_job059d_break_pops_catch_only_handler() {
        // Crossing a try with catch but no finally must pop its handler —
        // a stale handler would route the later index error into the loop's
        // own catch instead of the outer one.
        let output = run_source(
            "try {\n    while true {\n        try {\n            break\n        } catch e {\n            print(\"stale\")\n        }\n    }\n    let A = [10]\n    print(A[99])\n} catch e {\n    print(\"outer\")\n}",
        )
        .unwrap();
        assert_eq!(output, vec!["outer"]);
    }

    #[test]
    fn test_job059d_error_in_finally_during_break_chain_catches_outer() {
        // An error inside a chain finally abandons the remaining chain
        // markers (stack truncation) and propagates to the enclosing handler.
        let output = run_source(
            "try {\n    while true {\n        try {\n            break\n        } finally {\n            let A = [10]\n            print(A[99])\n        }\n    }\n} catch e {\n    print(\"caught\")\n}\nprint(\"after\")",
        )
        .unwrap();
        assert_eq!(output, vec!["caught", "after"]);
    }

    #[test]
    fn test_job059d_break_in_try_inside_function() {
        // Function bodies run on their own instruction stream; chain targets
        // must stay body-relative.
        let output = run_source(
            "function f() {\n    while true {\n        try {\n            print(\"b\")\n            break\n        } finally {\n            print(\"f\")\n        }\n    }\n    return 7\n}\nprint(f())",
        )
        .unwrap();
        assert_eq!(output, vec!["b", "f", "7"]);
    }

    #[test]
    fn test_job059d_return_from_finally_supersedes_break_chain() {
        // return inside the first chain finally replaces the chain: the outer
        // finally still runs (§43 return-through-finally), then the function
        // returns.
        let output = run_source(
            "function f() {\n    while true {\n        try {\n            try {\n                break\n            } finally {\n                print(\"inner\")\n                return 1\n            }\n        } finally {\n            print(\"outer\")\n        }\n    }\n    return 2\n}\nprint(f())",
        )
        .unwrap();
        assert_eq!(output, vec!["inner", "outer", "1"]);
    }

    #[test]
    fn test_job059d_break_unwinds_try_scope_before_finally() {
        // The try body's scope is unwound before the finally runs, so the
        // finally sees the shadowing outer binding — same as the normal path.
        let output = run_source(
            "let x = \"outer\"\nwhile true {\n    try {\n        let x = \"inner\"\n        break\n    } finally {\n        print(x)\n    }\n}\nprint(x)",
        )
        .unwrap();
        assert_eq!(output, vec!["outer", "outer"]);
    }

    // ---- Job 010: string.* ----

    #[test]
    fn test_job010_string_length() {
        let output = run_source(r#"print(string.length("Hello"))"#).unwrap();
        assert_eq!(output, vec!["5"]);
    }

    #[test]
    fn test_job010_string_length_wrong_type_error() {
        let err = run_source("print(string.length(10))").unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_contains_true() {
        let output = run_source(r#"print(string.contains("Hello World", "World"))"#).unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_job010_string_contains_false() {
        let output = run_source(r#"print(string.contains("Hello", "xyz"))"#).unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_job010_string_contains_wrong_type_error() {
        let err = run_source(r#"print(string.contains("Hello", 10))"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_trim() {
        let output = run_source(r#"print("[" + string.trim("  Hello  ") + "]")"#).unwrap();
        assert_eq!(output, vec!["[Hello]"]);
    }

    #[test]
    fn test_job010_string_trim_wrong_type_error() {
        let err = run_source("print(string.trim(true))").unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_split_spec_example() {
        let output = run_source(r#"print(string.split("a,b,c", ","))"#).unwrap();
        assert_eq!(output, vec!["[a, b, c]"]);
    }

    #[test]
    fn test_job010_string_split_wrong_type_error() {
        let err = run_source(r#"print(string.split(10, ","))"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_replace_spec_example() {
        let output = run_source(r#"print(string.replace("Hello World", "World", "ZeroScript"))"#)
            .unwrap();
        assert_eq!(output, vec!["Hello ZeroScript"]);
    }

    #[test]
    fn test_job010_string_replace_wrong_type_error() {
        let err = run_source(r#"print(string.replace("a", 1, "b"))"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_startswith_true() {
        let output = run_source(r#"print(string.startsWith("Hello World", "Hello"))"#).unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_job010_string_startswith_false() {
        let output = run_source(r#"print(string.startsWith("Hello", "World"))"#).unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_job010_string_startswith_wrong_type_error() {
        let err = run_source(r#"print(string.startsWith(10, "H"))"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_endswith_true() {
        let output = run_source(r#"print(string.endsWith("Hello World", "World"))"#).unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_job010_string_endswith_false() {
        let output = run_source(r#"print(string.endsWith("Hello", "World"))"#).unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_job010_string_endswith_wrong_type_error() {
        let err = run_source(r#"print(string.endsWith("Hello", 10))"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_lower() {
        let output = run_source(r#"print(string.lower("Hello World"))"#).unwrap();
        assert_eq!(output, vec!["hello world"]);
    }

    #[test]
    fn test_job010_string_lower_does_not_mutate_original() {
        let output = run_source(
            r#"let A = "Hello"
print(string.lower(A))
print(A)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["hello", "Hello"]);
    }

    #[test]
    fn test_job010_string_upper() {
        let output = run_source(r#"print(string.upper("Hello World"))"#).unwrap();
        assert_eq!(output, vec!["HELLO WORLD"]);
    }

    #[test]
    fn test_job010_string_upper_does_not_mutate_original() {
        let output = run_source(
            r#"let A = "Hello"
print(string.upper(A))
print(A)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["HELLO", "Hello"]);
    }

    #[test]
    fn test_job010_string_upper_wrong_type_error() {
        let err = run_source("print(string.upper(null))").unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_indexof_found() {
        let output = run_source(r#"print(string.indexOf("Hello World", "World"))"#).unwrap();
        assert_eq!(output, vec!["6"]);
    }

    #[test]
    fn test_job010_string_indexof_not_found() {
        let output = run_source(r#"print(string.indexOf("Hello", "xyz"))"#).unwrap();
        assert_eq!(output, vec!["-1"]);
    }

    #[test]
    fn test_job010_string_indexof_wrong_type_error() {
        let err = run_source(r#"print(string.indexOf("Hello", true))"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_string_ops_catchable() {
        let output = run_source(
            r#"try {
    print(string.length(10))
} catch e {
    print("caught")
}
print("after")"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught", "after"]);
    }

    #[test]
    fn test_job010_string_operations_do_not_change_original() {
        let output = run_source(
            r#"let A = "  Hello World  "
print(string.trim(A))
print(string.replace(A, "World", "Zero"))
print(string.lower(A))
print(A)"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec!["Hello World", "  Hello Zero  ", "  hello world  ", "  Hello World  "]
        );
    }

    // ---- Job 010: list.* ----

    #[test]
    fn test_job010_list_add() {
        let output = run_source(
            r#"let A = [10, 20]
list.add(A, 30)
print(A)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["[10, 20, 30]"]);
    }

    #[test]
    fn test_job010_list_add_wrong_type_error() {
        let err = run_source("list.add(10, 1)").unwrap_err();
        assert!(err.to_string().contains("must be list"), "{}", err);
    }

    #[test]
    fn test_job010_list_add_const_allowed() {
        let output = run_source("const A = [10]\nlist.add(A, 20)\nprint(A)").unwrap();
        assert_eq!(output, vec!["[10, 20]"]);
    }

    #[test]
    fn test_job010_list_remove() {
        let output = run_source(
            r#"let A = [10, 20, 30]
list.remove(A, 1)
print(A)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["[10, 30]"]);
    }

    #[test]
    fn test_job010_list_remove_invalid_index_error() {
        let err = run_source("let A = [10]\nlist.remove(A, 5)").unwrap_err();
        assert!(err.to_string().contains("out of bounds"), "{}", err);
    }

    #[test]
    fn test_job010_list_remove_const_allowed() {
        let output = run_source("const A = [10, 20]\nlist.remove(A, 0)\nprint(A)").unwrap();
        assert_eq!(output, vec!["[20]"]);
    }

    #[test]
    fn test_job010_list_remove_wrong_index_type_error() {
        let err = run_source("let A = [10]\nlist.remove(A, \"x\")").unwrap_err();
        assert!(err.to_string().contains("must be number"), "{}", err);
    }

    #[test]
    fn test_job010_list_length() {
        let output = run_source("print(list.length([10, 20, 30]))").unwrap();
        assert_eq!(output, vec!["3"]);
    }

    #[test]
    fn test_job010_list_length_empty() {
        let output = run_source("print(list.length([]))").unwrap();
        assert_eq!(output, vec!["0"]);
    }

    #[test]
    fn test_job010_list_length_wrong_type_error() {
        let err = run_source("print(list.length(\"abc\"))").unwrap_err();
        assert!(err.to_string().contains("must be list"), "{}", err);
    }

    #[test]
    fn test_job010_list_add_mutation_visible_to_caller_section35() {
        // Language §35 exact example
        let output = run_source(
            r#"function tambah(A) {
    list.add(A, 40)
}
let X = [10, 20, 30]
tambah(X)
print(X)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["[10, 20, 30, 40]"]);
    }

    #[test]
    fn test_job010_list_const_mutation_via_function_allowed() {
        // §35 (revised by ISSUES §58b): const is a binding rule, so mutation
        // through a function is allowed even when the source is a const binding.
        let output = run_source(
            r#"function tambah(A) {
    list.add(A, 40)
}
const X = [10]
tambah(X)
print(X)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["[10, 40]"]);
    }

    #[test]
    fn test_job010_list_add_catchable() {
        let output = run_source(
            r#"try {
    list.add("not a list", 20)
} catch e {
    print("caught")
}
print("after")"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught", "after"]);
    }

    // ---- Job 010: object.* ----

    #[test]
    fn test_job010_object_remove() {
        let output = run_source(
            r#"let user = { nama: "Kael", umur: 18 }
object.remove(user, "umur")
print(user)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["{nama: Kael}"]);
    }

    #[test]
    fn test_job010_object_remove_missing_key_error() {
        // §39 silent; analogous to §38 access → ERROR (ISSUES §60a)
        let err = run_source(r#"let o = { a: 1 }
object.remove(o, "missing")"#)
            .unwrap_err();
        assert!(err.to_string().contains("not found"), "{}", err);
    }

    #[test]
    fn test_job010_object_remove_non_string_key_error() {
        let err = run_source("let o = { a: 1 }\nobject.remove(o, 10)").unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job010_object_remove_const_allowed() {
        let output = run_source(r#"const o = { a: 1, b: 2 }
object.remove(o, "a")
print(o)"#)
            .unwrap();
        assert_eq!(output, vec!["{b: 2}"]);
    }

    #[test]
    fn test_job010_object_remove_wrong_type_error() {
        let err = run_source(r#"object.remove([1], "a")"#).unwrap_err();
        assert!(err.to_string().contains("must be object"), "{}", err);
    }

    #[test]
    fn test_job010_object_remove_catchable() {
        let output = run_source(
            r#"try {
    object.remove({ a: 1 }, "missing")
} catch e {
    print("caught")
}
print("after")"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught", "after"]);
    }

    #[test]
    fn test_job010_object_remove_mutation_visible_in_function() {
        let output = run_source(
            r#"function strip(o) {
    object.remove(o, "temp")
}
let data = { keep: 1, temp: 2 }
strip(data)
print(data)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["{keep: 1}"]);
    }

    // ---- Job 010: print / print.err ----

    #[test]
    fn test_job010_print_list_and_object() {
        let output = run_source("print([1, 2])\nprint({ a: 1 })").unwrap();
        assert_eq!(output, vec!["[1, 2]", "{a: 1}"]);
    }

    #[test]
    fn test_job010_print_function() {
        let output = run_source("function f() {}\nprint(f)").unwrap();
        assert_eq!(output.len(), 1);
        assert!(output[0].contains("function"), "{}", output[0]);
    }

    #[test]
    fn test_job010_print_null_boolean_number_string() {
        let output =
            run_source("print(null)\nprint(true)\nprint(10)\nprint(\"hi\")").unwrap();
        assert_eq!(output, vec!["null", "true", "10", "hi"]);
    }

    #[test]
    fn test_job010_print_err_outside_catch() {
        // §44: print.err only writes to error output, does not throw
        let vm = run_vm(r#"print.err("Something went wrong")
print("ok")"#)
            .unwrap();
        assert_eq!(vm.get_output(), vec!["ok"]);
        assert_eq!(vm.get_error_output(), vec!["Something went wrong"]);
    }

    #[test]
    fn test_job010_print_err_list_value() {
        let vm = run_vm(r#"print.err([1, 2])"#).unwrap();
        assert_eq!(vm.get_error_output(), vec!["[1, 2]"]);
    }

    #[test]
    fn test_job010_print_err_does_not_throw() {
        let output = run_source(
            r#"print.err("warn")
print("after")"#,
        )
        .unwrap();
        assert_eq!(output, vec!["after"]);
    }

    // ---- Job 010: regression / integration ----

    #[test]
    fn test_job010_string_split_result_is_list_length() {
        let output = run_source(r#"print(list.length(string.split("a,b,c", ",")))"#).unwrap();
        assert_eq!(output, vec!["3"]);
    }

    #[test]
    fn test_job010_redeclare_builtin_error() {
        let err = run_source("let string = 10").unwrap_err();
        assert!(err.to_string().contains("built-in"), "{}", err);
    }

    #[test]
    fn test_job010_unknown_method_error() {
        let err = run_source(r#"print(string.nope("x"))"#).unwrap_err();
        assert!(err.to_string().contains("not found"), "{}", err);
    }

    #[test]
    fn test_job010_list_add_string_element() {
        let output = run_source(r#"let A = []
list.add(A, "x")
print(A)"#)
            .unwrap();
        assert_eq!(output, vec!["[x]"]);
    }

    #[test]
    fn test_job010_spec_section42_examples() {
        let output = run_source(
            r#"print(string.length("Hello"))
print(string.contains("Hello World", "World"))
print(string.trim("  Hello  "))
print(string.split("a,b,c", ","))
print(string.replace("Hello World", "World", "ZeroScript"))"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec![
                "5",
                "true",
                "Hello",
                "[a, b, c]",
                "Hello ZeroScript"
            ]
        );
    }

    #[test]
    fn test_job010_spec_section58_examples() {
        let output = run_source(
            r#"let A = "Hello World"
print(string.startsWith(A, "Hello"))
print(string.endsWith(A, "World"))
print(string.lower(A))
print(string.upper(A))
print(string.indexOf(A, "World"))
print(A)"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec![
                "true",
                "true",
                "hello world",
                "HELLO WORLD",
                "6",
                "Hello World"
            ]
        );
    }

    #[test]
    fn test_job010_spec_section34_list_examples() {
        let output = run_source(
            r#"let A = [10, 20]
list.add(A, 30)
list.remove(A, 1)
print(A)
print(list.length(A))"#,
        )
        .unwrap();
        assert_eq!(output, vec!["[10, 30]", "2"]);
    }

    #[test]
    fn test_job010_spec_section39_object_remove_example() {
        let output = run_source(
            r#"let user = { nama: "Kael", umur: 18 }
object.remove(user, "umur")
print(user.nama)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["Kael"]);
    }

    #[test]
    fn test_job010_spec_section44_console() {
        let vm = run_vm(
            r#"print("Hello")
print(10)
print(true)
print(null)
print.err("Something went wrong")"#,
        )
        .unwrap();
        assert_eq!(vm.get_output(), vec!["Hello", "10", "true", "null"]);
        assert_eq!(vm.get_error_output(), vec!["Something went wrong"]);
    }

    // ---- Job 011: color.* (§45) ----

    #[test]
    fn test_job011_color_red() {
        let output = run_source(r#"print(color.red("Error"))"#).unwrap();
        assert_eq!(output, vec!["\x1b[31mError\x1b[39m"]);
    }

    #[test]
    fn test_job011_color_green() {
        let output = run_source(r#"print(color.green("Success"))"#).unwrap();
        assert_eq!(output, vec!["\x1b[32mSuccess\x1b[39m"]);
    }

    #[test]
    fn test_job011_color_yellow() {
        let output = run_source(r#"print(color.yellow("Warning"))"#).unwrap();
        assert_eq!(output, vec!["\x1b[33mWarning\x1b[39m"]);
    }

    #[test]
    fn test_job011_color_blue() {
        let output = run_source(r#"print(color.blue("Info"))"#).unwrap();
        assert_eq!(output, vec!["\x1b[34mInfo\x1b[39m"]);
    }

    #[test]
    fn test_job011_color_bold_alone() {
        let output = run_source(r#"print(color.bold("Hi"))"#).unwrap();
        assert_eq!(output, vec!["\x1b[1mHi\x1b[22m"]);
    }

    #[test]
    fn test_job011_color_bold_combines_with_red() {
        // §45: color.bold(color.red("Error")) must nest correctly
        let output = run_source(r#"print(color.bold(color.red("Error")))"#).unwrap();
        assert_eq!(output, vec!["\x1b[1m\x1b[31mError\x1b[39m\x1b[22m"]);
    }

    #[test]
    fn test_job011_color_wrong_arity_error() {
        let err = run_source("print(color.red())").unwrap_err();
        assert!(err.to_string().contains("exactly 1"), "{}", err);
    }

    #[test]
    fn test_job011_color_catchable() {
        let output = run_source(
            r#"try {
    print(color.red())
} catch e {
    print("caught")
}"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// ISSUES §61i: color accepts any single argument, rendered like print.
    #[test]
    fn test_issue61i_color_accepts_non_string_argument() {
        let output = run_source(
            r#"print(color.red(10))
print(color.blue(true))
print(color.green([1, 2]))"#,
        )
        .unwrap();
        assert_eq!(output[0], "\x1b[31m10\x1b[39m");
        assert_eq!(output[1], "\x1b[34mtrue\x1b[39m");
        assert_eq!(output[2], "\x1b[32m[1, 2]\x1b[39m");
    }

    // ---- Job 011: file.* (§48) ----

    fn temp_path(name: &str) -> String {
        let dir = std::env::temp_dir().join(format!("zeroscript_job011_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name).to_string_lossy().into_owned()
    }

    fn cleanup(path: &str) {
        let p = std::path::Path::new(path);
        if p.is_dir() {
            let _ = std::fs::remove_dir_all(p);
        } else {
            let _ = std::fs::remove_file(p);
        }
    }

    fn run_source_with_args(source: &str, args: Vec<String>) -> Result<Vec<String>, VmError> {
        let bc = compile_source(source);
        let mut vm = ZeroVM::new(bc);
        vm.set_program_args(args);
        vm.run()?;
        Ok(vm.get_output().to_vec())
    }

    #[test]
    fn test_job011_file_write_read_roundtrip() {
        let path = temp_path("roundtrip.txt");
        let src = format!(
            r#"file.write("{path}", "Hello ZeroScript")
let content = file.read("{path}")
print(content)"#,
            path = path.replace('\\', "\\\\")
        );
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["Hello ZeroScript"]);
        cleanup(&path);
    }

    #[test]
    fn test_job011_file_exists_true_false() {
        let path = temp_path("exists.txt");
        let src = format!(
            r#"print(file.exists("{path}"))
file.write("{path}", "x")
print(file.exists("{path}"))"#,
            path = path.replace('\\', "\\\\")
        );
        let _ = std::fs::remove_file(&path);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["false", "true"]);
        cleanup(&path);
    }

    #[test]
    fn test_job011_file_remove_then_not_exists() {
        let path = temp_path("remove.txt");
        let src = format!(
            r#"file.write("{path}", "x")
file.remove("{path}")
print(file.exists("{path}"))"#,
            path = path.replace('\\', "\\\\")
        );
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["false"]);
        cleanup(&path);
    }

    #[test]
    fn test_job011_file_mkdir() {
        let path = temp_path("makedir");
        let _ = std::fs::remove_dir_all(&path);
        let src = format!(
            r#"file.mkdir("{path}")
print(file.exists("{path}"))"#,
            path = path.replace('\\', "\\\\")
        );
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["true"]);
        cleanup(&path);
    }

    /// ISSUES §61b: side-effecting file mutators return null.
    #[test]
    fn test_issue61b_file_mutators_return_null() {
        let dir = temp_path("ret61b");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = format!(
            r#"let w = file.write("{dir}/a.txt", "x")
let r = file.remove("{dir}/a.txt")
let m = file.mkdir("{dir}/sub")
print(w == null)
print(r == null)
print(m == null)"#,
            dir = dir.replace('\\', "\\\\")
        );
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["true", "true", "true"]);
        cleanup(&dir);
    }

    /// ISSUES §61c: file.mkdir is single-level — missing parent → catchable error.
    #[test]
    fn test_issue61c_file_mkdir_missing_parent_is_error() {
        let base = temp_path("mkdir61c");
        let _ = std::fs::remove_dir_all(&base);
        let src = format!(
            r#"try {{
    file.mkdir("{base}/child")
    print("no-error")
}} catch e {{
    print("caught")
}}"#,
            base = base.replace('\\', "\\\\")
        );
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
        cleanup(&base);
    }

    #[test]
    fn test_job011_file_read_missing_catchable() {
        let path = temp_path("does_not_exist.txt");
        let _ = std::fs::remove_file(&path);
        let src = format!(
            r#"try {{
    file.read("{path}")
    print("no-error")
}} catch e {{
    print("caught")
}}"#,
            path = path.replace('\\', "\\\\")
        );
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    #[test]
    fn test_job011_file_read_missing_uncatchable_surfaces_error() {
        let path = temp_path("also_missing.txt");
        let _ = std::fs::remove_file(&path);
        let src = format!(r#"file.read("{path}")"#, path = path.replace('\\', "\\\\"));
        let err = run_source(&src).unwrap_err();
        assert!(err.to_string().contains("file.read"), "{}", err);
    }

    #[test]
    fn test_job011_file_write_wrong_type_error() {
        let err = run_source(r#"file.write(10, "x")"#).unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    // ---- Job 011: system.* (§52) ----

    #[test]
    fn test_job011_system_args_excludes_program_name() {
        // §52: zerovm app.zs hello world → ["hello", "world"]
        let output =
            run_source_with_args("print(system.args())", vec!["hello".into(), "world".into()])
                .unwrap();
        assert_eq!(output, vec!["[hello, world]"]);
    }

    #[test]
    fn test_job011_system_args_default_empty() {
        let output = run_source("print(system.args())").unwrap();
        assert_eq!(output, vec!["[]"]);
    }

    #[test]
    fn test_job011_system_env_existing() {
        // HOME is set in the test environment
        let output = run_source(r#"let v = system.env("HOME")
print(v == null)"#)
            .unwrap();
        assert_eq!(output, vec!["false"]);
    }

    #[test]
    fn test_job011_system_env_missing_returns_null() {
        let output =
            run_source(r#"print(system.env("ZEROSCRIPT_NO_SUCH_VAR_XYZ_12345") === null)"#)
                .unwrap();
        assert_eq!(output, vec!["true"]);
    }

    #[test]
    fn test_job011_system_os_returns_string() {
        let output = run_source(r#"print(type(system.os()))"#).unwrap();
        assert_eq!(output, vec!["string"]);
    }

    #[test]
    fn test_job011_system_arch_returns_string() {
        let output = run_source(r#"print(type(system.arch()))"#).unwrap();
        assert_eq!(output, vec!["string"]);
    }

    #[test]
    fn test_job011_system_os_arch_nonempty() {
        let output = run_source(r#"print(system.os() != "")
print(system.arch() != "")"#)
            .unwrap();
        assert_eq!(output, vec!["true", "true"]);
    }

    // ---- Job 011: json.* (§51) ----

    #[test]
    fn test_job011_json_stringify_parse_roundtrip_object() {
        let output = run_source(
            r#"let obj = { nama: "Kael", umur: 18, aktif: true }
let s = json.stringify(obj)
print(s)
let back = json.parse(s)
print(back.nama)
print(back.umur)
print(back.aktif)"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec![
                r#"{"nama":"Kael","umur":18,"aktif":true}"#,
                "Kael",
                "18",
                "true"
            ]
        );
    }

    #[test]
    fn test_job011_json_stringify_parse_roundtrip_list() {
        let output = run_source(
            r#"let xs = [1, "two", false, null]
let s = json.stringify(xs)
print(s)
let back = json.parse(s)
print(back)
print(list.length(back))"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec![r#"[1,"two",false,null]"#, r#"[1, two, false, null]"#, "4"]
        );
    }

    #[test]
    fn test_job011_json_parse_spec_example() {
        // §51: explicit parse only — no automatic JSON detection
        let output = run_source(
            r#"let body = "{\"nama\":\"Kael\"}"
let data = json.parse(body)
print(data.nama)"#,
        )
        .unwrap();
        assert_eq!(output, vec!["Kael"]);
    }

    #[test]
    fn test_job011_json_stringify_spec_example() {
        let output = run_source(
            r#"let body = json.stringify({ nama: "Kael" })
print(body)"#,
        )
        .unwrap();
        assert_eq!(output, vec![r#"{"nama":"Kael"}"#]);
    }

    #[test]
    fn test_job011_json_parse_invalid_catchable() {
        let output = run_source(
            r#"try {
    json.parse("not valid json")
    print("no-error")
} catch e {
    print("caught")
}
print("after")"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught", "after"]);
    }

    #[test]
    fn test_job011_json_parse_invalid_uncatchable_surfaces() {
        let err = run_source(r#"json.parse("{bad")"#).unwrap_err();
        assert!(err.to_string().contains("Invalid JSON"), "{}", err);
    }

    #[test]
    fn test_job011_json_parse_wrong_type_error() {
        let err = run_source("json.parse(10)").unwrap_err();
        assert!(err.to_string().contains("must be string"), "{}", err);
    }

    #[test]
    fn test_job011_json_stringify_nested_object_list() {
        let output = run_source(
            r#"let data = { user: { name: "Kael" }, tags: ["a", "b"], n: 3.5 }
print(json.stringify(data))"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec![r#"{"user":{"name":"Kael"},"tags":["a","b"],"n":3.5}"#]
        );
    }

    #[test]
    fn test_job011_json_parse_returns_null_for_json_null() {
        let output = run_source(r#"print(json.parse("null") === null)"#).unwrap();
        assert_eq!(output, vec!["true"]);
    }

    /// ISSUES §61g: json.stringify accepts top-level scalars (roundtrip with json.parse).
    #[test]
    fn test_issue61g_json_stringify_top_level_scalars() {
        let output = run_source(
            r#"print(json.stringify(42))
print(json.stringify("hi"))
print(json.stringify(true))
print(json.stringify(null))"#,
        )
        .unwrap();
        assert_eq!(output, vec!["42", "\"hi\"", "true", "null"]);
    }

    /// ISSUES §61g: function values and non-finite numbers are rejected with catchable errors.
    #[test]
    fn test_issue61g_json_stringify_rejects_function_and_non_finite() {
        let output = run_source(
            r#"function f() { return 1 }
try {
    json.stringify(f)
    print("no-error")
} catch e {
    print("caught")
}
try {
    json.stringify(number("1e400"))
    print("no-error")
} catch e {
    print("caught")
}"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught", "caught"]);
    }

    /// ISSUES §61h: duplicate keys — last value wins, first-seen key order preserved.
    #[test]
    fn test_issue61h_json_duplicate_keys_last_wins() {
        let output = run_source(
            r#"let v = json.parse("{\"a\": 1, \"b\": 9, \"a\": 2}")
print(v.a)
print(v.b)
print(json.stringify(v))"#,
        )
        .unwrap();
        assert_eq!(output, vec!["2", "9", r#"{"a":2,"b":9}"#]);
    }

    // ---- Job 011: namespace integration ----

    #[test]
    fn test_job011_redeclare_namespace_builtin_error() {
        let err = run_source("let color = 10").unwrap_err();
        assert!(err.to_string().contains("built-in"), "{}", err);
    }

    #[test]
    fn test_job011_unknown_method_error() {
        let err = run_source(r#"print(file.nope("x"))"#).unwrap_err();
        assert!(err.to_string().contains("not found"), "{}", err);
    }

    #[test]
    fn test_job011_spec_section45_examples() {
        let output = run_source(
            r#"print(color.red("Error"))
print(color.green("Success"))
print(color.yellow("Warning"))
print(color.blue("Info"))
print(color.bold(color.red("Error")))"#,
        )
        .unwrap();
        assert_eq!(output.len(), 5);
        assert!(output[0].contains("Error"));
        assert!(output[1].contains("Success"));
        assert!(output[2].contains("Warning"));
        assert!(output[3].contains("Info"));
        assert!(output[4].starts_with("\x1b[1m"));
        assert!(output[4].contains("\x1b[31m"));
        assert!(output[4].ends_with("\x1b[22m"));
    }

    #[test]
    fn test_job011_spec_section52_examples() {
        let output = run_source_with_args(
            r#"print(type(system.args()))
print(type(system.os()))
print(type(system.arch()))"#,
            vec!["hello".into(), "world".into()],
        )
        .unwrap();
        assert_eq!(output, vec!["list", "string", "string"]);
    }

    // ---- Job 014: async/await core (§49, §101, §106-§108, §113) ----

    fn compile_err_msg(source: &str) -> String {
        let mut lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();
        let mut parser = Parser::new(tokens);
        match parser.parse() {
            Ok(program) => match compile(&program) {
                Ok(_) => panic!("Expected compile error for: {}", source),
                Err(e) => e.to_string(),
            },
            Err(e) => e.to_string(),
        }
    }

    fn run_vm_source(source: &str) -> ZeroVM {
        let bc = compile_source(source);
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        vm
    }

    /// Req 1 + 3: async function call creates a Task with initial state
    /// "pending"; the task value is a distinct runtime value (§101, §106).
    #[test]
    fn test_job014_async_call_creates_pending_task() {
        let vm = run_vm_source(
            r#"async function f() { return 1 }
let t = f()
print(type(t))
print(task.state(t))"#,
        );
        assert_eq!(vm.get_output(), &["task".to_string(), "pending".to_string()]);
        assert_eq!(vm.tasks.len(), 1);
        // Scheduler drained the un-awaited task after the top level (§112).
        assert_eq!(vm.tasks[0].state, TaskState::Completed);
        assert_eq!(vm.tasks[0].result, Some(Value::Number(1.0)));
    }

    /// Req 2: `await` outside an async function → compile-time ERROR (§49, §101).
    #[test]
    fn test_job014_await_outside_async_is_compile_error() {
        // Top level.
        let msg = compile_err_msg("async function f() { return 1 }\nlet t = f()\nlet x = await t");
        assert!(msg.contains("await"), "{}", msg);

        // Inside a plain (non-async) function.
        let msg = compile_err_msg("function syncFn() { await other() }");
        assert!(msg.contains("await"), "{}", msg);

        // Nested sync function inside an async function resets the context.
        let msg =
            compile_err_msg("async function outer() { function inner() { await nope() } }");
        assert!(msg.contains("await"), "{}", msg);
    }

    /// Req 4: `await task` returns the result once the task is completed
    /// (§107 example: tambah(10, 20) → 30).
    #[test]
    fn test_job014_await_returns_result() {
        let output = run_source(
            r#"async function tambah(A, B) { return A + B }
async function main() {
    let taskA = tambah(10, 20)
    let result = await taskA
    print(result)
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["30"]);
    }

    /// Req 5: await of a failed task forwards the stored error; try/catch
    /// handles it (§49, §108).
    #[test]
    fn test_job014_await_failed_task_catchable() {
        let bc = compile_source(
            r#"async function gagal() { return nosuchvar }
async function main() {
    let taskA = gagal()
    try {
        let result = await taskA
        print(result)
    } catch e {
        print.err(e)
    }
}
main()"#,
        );
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert!(vm.get_output().is_empty());
        assert_eq!(vm.get_error_output().len(), 1);
        assert!(
            vm.get_error_output()[0].contains("Undefined variable 'nosuchvar'"),
            "{}",
            vm.get_error_output()[0]
        );
        // tasks[0] = main (completed after catching), tasks[1] = gagal (failed).
        assert_eq!(vm.tasks[0].state, TaskState::Completed);
        assert_eq!(vm.tasks[1].state, TaskState::Failed);
    }

    /// Req 7: task.state() reports the correct string at every observable
    /// phase (§106). "cancelled" is unreachable until Job 015's task.cancel.
    #[test]
    fn test_job014_task_state_transitions() {
        let vm = run_vm_source(
            r#"async function f() { print(task.state(t))
    return 1 }
async function bad() { return nosuchvar }
async function check() { print(task.state(t))
    print(task.state(b)) }
let t = f()
let b = bad()
print(task.state(t))
let c = check()"#,
        );
        assert_eq!(
            vm.get_output(),
            &[
                "pending".to_string(),
                "running".to_string(),
                "completed".to_string(),
                "failed".to_string()
            ]
        );
        assert_eq!(vm.tasks[0].state, TaskState::Completed);
        assert_eq!(vm.tasks[1].state, TaskState::Failed);
        assert_eq!(vm.tasks[2].state, TaskState::Completed);
    }

    /// Req 8: awaiting a completed task again returns the stored result
    /// without re-running the body (§101, §107, §113).
    #[test]
    fn test_job014_await_twice_does_not_rerun() {
        let output = run_source(
            r#"async function f() { print("once")
    return 1 }
async function main() {
    let t = f()
    let a = await t
    let b = await t
    print(a)
    print(b)
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["once", "1", "1"]);
    }

    /// Req 9: nested async calls (outer awaits inner).
    #[test]
    fn test_job014_nested_async_calls() {
        let output = run_source(
            r#"async function inner() { return 2 }
async function outer() {
    let t = inner()
    let v = await t
    return v + 1
}
async function main() {
    let t = outer()
    let r = await t
    print(r)
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["3"]);
    }

    /// Req 10: recursion inside an async function.
    #[test]
    fn test_job014_async_recursion() {
        let output = run_source(
            r#"async function f(n) {
    if (n <= 1) { return 1 }
    let t = f(n - 1)
    let sub = await t
    return n * sub
}
async function main() {
    let t = f(5)
    let r = await t
    print(r)
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["120"]);
    }

    /// §106: task.state(A) with a non-task argument → ERROR.
    #[test]
    fn test_job014_task_state_non_task_error() {
        let err = run_source("print(task.state(5))").unwrap_err();
        assert!(err.to_string().contains("task"), "{}", err);
    }

    /// Part D decision (spec silent on awaiting non-tasks): runtime ERROR at
    /// the await point, catchable like any other runtime error. See
    /// ISSUES.md §63c.
    #[test]
    fn test_job014_await_non_task_is_error() {
        let bc = compile_source(
            r#"async function main() {
    try {
        let x = await 5
        print(x)
    } catch e {
        print.err(e)
    }
}
main()"#,
        );
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert!(vm.get_output().is_empty());
        assert_eq!(vm.get_error_output().len(), 1);
        assert!(
            vm.get_error_output()[0].contains("await expects a task"),
            "{}",
            vm.get_error_output()[0]
        );
    }

    /// §108: a task that fails without ever being awaited stores its error and
    /// does not stop the program or other tasks.
    #[test]
    fn test_job014_unawaited_failure_does_not_stop_program() {
        let vm = run_vm_source(
            r#"async function bad() { return nosuchvar }
async function good() { print("good") }
let b = bad()
let g = good()
print("done")"#,
        );
        assert_eq!(vm.get_output(), &["done".to_string(), "good".to_string()]);
        assert_eq!(vm.tasks[0].state, TaskState::Failed);
        assert_eq!(vm.tasks[1].state, TaskState::Completed);
    }

    /// §108 "Un-awaited Task Failure": one stderr warning naming the failed
    /// task and its error, without touching stdout.
    #[test]
    fn test_job014_unawaited_failure_emits_warning() {
        let vm = run_vm_source(
            r#"async function bad() { return nosuchvar }
let b = bad()
print("done")"#,
        );
        assert_eq!(vm.get_output(), &["done".to_string()]);
        let err = vm.get_error_output();
        assert_eq!(err.len(), 1, "{:?}", err);
        assert!(
            err[0].starts_with("ZeroScript warning:"),
            "unexpected warning: {}",
            err[0]
        );
        assert!(err[0].contains("task \"bad\""), "{}", err[0]);
        assert!(
            err[0].contains("Undefined variable 'nosuchvar'"),
            "{}",
            err[0]
        );
    }

    /// §108: exactly one warning per un-awaited failed task.
    #[test]
    fn test_job014_unawaited_failure_one_warning_per_task() {
        let vm = run_vm_source(
            r#"async function bad() { return nosuchvar }
let a = bad()
let b = bad()
print("done")"#,
        );
        assert_eq!(vm.get_output(), &["done".to_string()]);
        let err = vm.get_error_output();
        assert_eq!(err.len(), 2, "{:?}", err);
        assert!(err[0].contains("task \"bad\"") && err[1].contains("task \"bad\""));
    }

    /// §108: an awaited failure — even one caught by try/catch — produces no
    /// warning; a failed task whose error was observed counts as handled.
    #[test]
    fn test_job014_awaited_failure_emits_no_warning() {
        let vm = run_vm_source(
            r#"async function bad() { return nosuchvar }
async function main() {
    try {
        let r = await bad()
        print(r)
    } catch e {
        print("caught")
    }
}
main()"#,
        );
        assert_eq!(vm.get_output(), &["caught".to_string()]);
        assert!(
            vm.get_error_output().is_empty(),
            "{:?}",
            vm.get_error_output()
        );
    }

    /// §108 + user option B (2026-10-02): a failure delivered through an
    /// awaited task.all counts as observed — §108's own example emits no
    /// warning even though the member task was never awaited directly.
    #[test]
    fn test_job014_task_all_observed_failure_emits_no_warning() {
        let vm = run_vm_source(
            r#"async function gagal() { return 10 / 0 }
async function main() {
    try {
        let r = await task.all([gagal()])
        print(r)
    } catch e {
        print("caught")
    }
}
main()"#,
        );
        assert_eq!(vm.get_output(), &["caught".to_string()]);
        assert!(
            vm.get_error_output().is_empty(),
            "{:?}",
            vm.get_error_output()
        );
    }

    /// §108: a program with no failed tasks emits no warnings.
    #[test]
    fn test_job014_no_failures_emits_no_warning() {
        let vm = run_vm_source(
            r#"async function ok() { return 1 }
async function main() {
    let r = await ok()
    print(r)
}
main()"#,
        );
        assert_eq!(vm.get_output(), &["1".to_string()]);
        assert!(
            vm.get_error_output().is_empty(),
            "{:?}",
            vm.get_error_output()
        );
    }

    /// §49: an error inside a task body can be handled by try/catch INSIDE the
    /// task — the task itself then completes normally.
    #[test]
    fn test_job014_try_catch_inside_task_body() {
        let bc = compile_source(
            r#"async function f() {
    try {
        let x = nosuchvar
        print(x)
    } catch e {
        print.err(e)
    }
    return 1
}
async function main() {
    let t = f()
    let r = await t
    print(r)
}
main()"#,
        );
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_output(), vec!["1"]);
        assert_eq!(vm.get_error_output().len(), 1);
        assert!(
            vm.get_error_output()[0].contains("Undefined variable 'nosuchvar'"),
            "{}",
            vm.get_error_output()[0]
        );
        // The task did not fail: its own try/catch handled the error.
        assert_eq!(vm.tasks[1].state, TaskState::Completed);
    }

    /// §113: awaiting a failed task again re-raises the stored error (the task
    /// stays terminal and is not re-run).
    #[test]
    fn test_job014_second_await_of_failed_task_reraises() {
        let bc = compile_source(
            r#"async function bad() { return nosuchvar }
async function main() {
    let t = bad()
    try {
        let a = await t
        print(a)
    } catch e {
        print.err("first")
    }
    try {
        let b = await t
        print(b)
    } catch e {
        print.err("second")
    }
}
main()"#,
        );
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert!(vm.get_output().is_empty());
        assert_eq!(vm.get_error_output(), &["first".to_string(), "second".to_string()]);
        assert_eq!(vm.tasks[1].state, TaskState::Failed);
    }

    /// Async closure call path: an async function that captured variables is
    /// called as a closure; the task keeps the env for its body (§101).
    #[test]
    fn test_job014_async_closure_capture() {
        let output = run_source(
            r#"function outer() {
    let base = 10
    async function inner(n) { print(base + n) }
    let t = inner(5)
}
outer()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["15"]);
    }

    // ==================== Job 015: Async Concurrency ====================

    /// Req 1 (§102): several tasks are created before any await; both
    /// progress concurrently — the short task reaches a terminal state while
    /// the long task is still being awaited, and each result stays separate.
    #[test]
    fn test_job015_concurrent_creation_before_await() {
        let output = run_source(
            r#"async function ambilUser() { await task.sleep(60)
    return "user" }
async function ambilData() { await task.sleep(10)
    return "data" }
async function main() {
    let userTask = ambilUser()
    let dataTask = ambilData()
    let user = await userTask
    print(user)
    print(task.state(dataTask))
    let data = await dataTask
    print(data)
}
main()"#,
        )
        .unwrap();
        // dataTask (10 ms) completed while main was still awaiting
        // userTask (60 ms): creating task A first does not gate task B.
        assert_eq!(output, vec!["user", "completed", "data"]);
    }

    /// Req 2 (§103): task.all() waits for every task and returns results in
    /// input order, not completion order.
    #[test]
    fn test_job015_all_results_input_order() {
        let output = run_source(
            r#"async function slow() { await task.sleep(60)
    return "slow" }
async function fast() { await task.sleep(10)
    return "fast" }
async function main() {
    let results = await task.all([slow(), fast()])
    print(results[0])
    print(results[1])
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["slow", "fast"]);
    }

    /// Req 3 (§103): task.all([]) → empty list.
    #[test]
    fn test_job015_all_empty_list() {
        let output = run_source(
            r#"async function main() {
    let results = await task.all([])
    print(type(results))
    print(list.length(results))
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["list", "0"]);
    }

    /// Req 4 (§103): one failing task fails task.all() (fail-fast, error
    /// forwarded) and does not cancel or corrupt the other tasks.
    #[test]
    fn test_job015_all_fail_fast() {
        let output = run_source(
            r#"async function bad() { return nosuchvar }
async function slow() { await task.sleep(200)
    return "ok" }
async function main() {
    let s = slow()
    try {
        await task.all([bad(), s])
        print("no error")
    } catch e {
        print("caught")
        print(task.state(s))
    }
}
main()"#,
        )
        .unwrap();
        // The slow task is untouched by the sibling failure: still running.
        assert_eq!(output, vec!["caught", "running"]);
    }

    /// Req 5 (§103): a non-task element in the list → ERROR.
    #[test]
    fn test_job015_all_non_task_error() {
        let output = run_source(
            r#"async function main() {
    try {
        await task.all([1, 2])
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// Req 6 (§104): task.race() yields the first finisher's result; losing
    /// tasks are not cancelled (they keep running).
    #[test]
    fn test_job015_race_first_finisher_wins() {
        let output = run_source(
            r#"async function slow() { await task.sleep(60)
    return "slow" }
async function fast() { await task.sleep(10)
    return "fast" }
async function main() {
    let s = slow()
    let f = fast()
    let winner = await task.race([s, f])
    print(winner)
    print(task.state(s))
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["fast", "running"]);
    }

    /// Req 7 (§104): if the first finisher fails, task.race() forwards the
    /// error to the awaiter.
    #[test]
    fn test_job015_race_first_finisher_error() {
        let output = run_source(
            r#"async function bad() { return nosuchvar }
async function slow() { await task.sleep(200)
    return "slow" }
async function main() {
    try {
        await task.race([bad(), slow()])
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// Req 8 (§104): task.race([]) → ERROR (nothing to wait for).
    #[test]
    fn test_job015_race_empty_error() {
        let output = run_source(
            r#"async function main() {
    try {
        await task.race([])
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// Req 9 (§105): task.cancel() on a running task → state becomes
    /// "cancelled".
    #[test]
    fn test_job015_cancel_running_task() {
        let output = run_source(
            r#"async function sleepy() { await task.sleep(1000)
    print("never") }
async function main() {
    let t = sleepy()
    await task.sleep(0)
    print(task.state(t))
    task.cancel(t)
    print(task.state(t))
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["running", "cancelled"]);
    }

    /// Req 10 (§105): task.cancel() on a completed task is a no-op — no
    /// error, state unchanged.
    #[test]
    fn test_job015_cancel_completed_noop() {
        let output = run_source(
            r#"async function quick() { return 1 }
async function main() {
    let t = quick()
    let r = await t
    task.cancel(t)
    print(task.state(t))
    print(r)
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["completed", "1"]);
    }

    /// Req 11 (§105): awaiting a cancelled task raises a cancellation error
    /// that try/catch handles.
    #[test]
    fn test_job015_await_cancelled_is_error() {
        let output = run_source(
            r#"async function sleepy() { await task.sleep(1000)
    return "never" }
async function main() {
    let t = sleepy()
    await task.sleep(0)
    task.cancel(t)
    try {
        await t
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// Req 12 (§109): task.wait() on completed → Null (result not taken);
    /// on failed → the task error; on cancelled → the cancellation error.
    #[test]
    fn test_job015_wait_terminal_states() {
        let output = run_source(
            r#"async function ok() { return 1 }
async function bad() { return nosuchvar }
async function sleepy() { await task.sleep(1000)
    return "never" }
async function main() {
    let a = ok()
    let b = bad()
    let c = sleepy()
    let w = await task.wait(a)
    print(type(w))
    try {
        await task.wait(b)
        print("no error")
    } catch e {
        print("wait failed caught")
    }
    task.cancel(c)
    try {
        await task.wait(c)
        print("no error")
    } catch e {
        print("wait cancelled caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec!["null", "wait failed caught", "wait cancelled caught"]
        );
    }

    /// Req 13 (§110): task finishes before the limit → normal result.
    #[test]
    fn test_job015_timeout_success() {
        let output = run_source(
            r#"async function quick() { await task.sleep(10)
    return "ok" }
async function main() {
    let t = quick()
    let r = await task.timeout(t, 5000)
    print(r)
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["ok"]);
    }

    /// Req 14 (§110): timeout → ERROR, and the timed-out task is NOT
    /// cancelled — it keeps running and can still finish normally.
    #[test]
    fn test_job015_timeout_error_task_keeps_running() {
        let output = run_source(
            r#"async function slow() { await task.sleep(500)
    print("slow done")
    return "slow" }
async function main() {
    let t = slow()
    try {
        await task.timeout(t, 10)
        print("no timeout")
    } catch e {
        print("timeout caught")
    }
    print(task.state(t))
    await task.wait(t)
    print("finished")
}
main()"#,
        )
        .unwrap();
        assert_eq!(
            output,
            vec!["timeout caught", "running", "slow done", "finished"]
        );
    }

    /// Req 15 (§111, §112): completion order follows sleep duration, not
    /// creation order — mirrors the §112 example (1000 ms vs 500 ms).
    #[test]
    fn test_job015_sleep_ordering_not_creation_order() {
        let output = run_source(
            r#"async function A() { await task.sleep(1000)
    print("A") }
async function B() { await task.sleep(500)
    print("B") }
async function main() {
    let taskA = A()
    let taskB = B()
    await task.all([taskA, taskB])
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["B", "A"]);
    }

    /// Req 16 (§111): task.sleep(0) does not crash or hang; it yields to
    /// other runnable tasks before resuming.
    #[test]
    fn test_job015_sleep_zero_yields() {
        let output = run_source(
            r#"async function other() { print("other") }
async function main() {
    let o = other()
    await task.sleep(0)
    print("main")
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["other", "main"]);
    }

    /// Extra: task.cancel() on a non-task argument → ERROR (§105 requires
    /// an asynchronous task).
    #[test]
    fn test_job015_cancel_non_task_error() {
        let output = run_source(
            r#"async function main() {
    try {
        task.cancel(42)
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// Extra (ISSUES §64b): cancelling the currently running task from
    /// inside its own body → runtime ERROR (spec silent; a single-threaded
    /// scheduler cannot unwind itself).
    #[test]
    fn test_job015_self_cancel_is_error() {
        let output = run_source(
            r#"async function selfy() { task.cancel(t)
    print("after") }
let t = selfy()
async function main() {
    try {
        await t
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    /// Extra (ISSUES §64c): a cycle of tasks awaiting each other makes no
    /// progress and has no timers → runtime ERROR instead of a hang
    /// (spec silent on await cycles).
    #[test]
    fn test_job015_await_cycle_deadlock_error() {
        let err = run_source(
            r#"async function a() { await tb }
async function b() { await ta }
let ta = a()
let tb = b()
async function main() { await ta }
main()"#,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("deadlock"),
            "unexpected error: {}",
            err
        );
    }

    /// Extra: negative sleep duration → ERROR (§111: non-negative integer).
    #[test]
    fn test_job015_sleep_negative_error() {
        let output = run_source(
            r#"async function main() {
    try {
        await task.sleep(-1)
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#,
        )
        .unwrap();
        assert_eq!(output, vec!["caught"]);
    }

    // ==================== Job 016: Standard Library HTTP ====================

    /// One request recorded by the deterministic local mock server.
    struct RecordedRequest {
        method: String,
        target: String,
        headers: Vec<(String, String)>,
        body: String,
    }

    struct MockReply {
        status: u16,
        headers: Vec<(String, String)>,
        body: String,
        /// Accept the request but never respond (timeout tests).
        hang: bool,
    }

    /// Start a localhost HTTP/1.1 server that records every request and
    /// answers with `handler`'s reply. Returns the base URL and the shared
    /// recording log. Handlers run inline in the accept loop (tests are
    /// sequential), `hang` sleeps bounded before dropping the connection.
    fn spawn_mock_server<F>(handler: F) -> (String, Arc<Mutex<Vec<RecordedRequest>>>)
    where
        F: Fn(&RecordedRequest) -> MockReply + Send + 'static,
    {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let records: Arc<Mutex<Vec<RecordedRequest>>> = Arc::new(Mutex::new(Vec::new()));
        let rec2 = records.clone();
        thread::spawn(move || {
            for conn in listener.incoming() {
                let Ok(mut stream) = conn else { continue };
                let mut buf: Vec<u8> = Vec::new();
                let mut chunk = [0u8; 1024];
                let header_end = loop {
                    if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                        break Some(p);
                    }
                    match stream.read(&mut chunk) {
                        Ok(0) => break None,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        Err(_) => break None,
                    }
                    if buf.len() > 64 * 1024 {
                        break None;
                    }
                };
                let Some(header_end) = header_end else { continue };
                let head = String::from_utf8_lossy(&buf[..header_end]).to_string();
                let mut lines = head.split("\r\n");
                let request_line = lines.next().unwrap_or("");
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let target = parts.next().unwrap_or("").to_string();
                let mut headers = Vec::new();
                for line in lines {
                    if let Some((k, v)) = line.split_once(':') {
                        headers.push((k.trim().to_string(), v.trim().to_string()));
                    }
                }
                let content_length = headers
                    .iter()
                    .find(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, v)| v.parse::<usize>().ok())
                    .unwrap_or(0);
                let mut body = buf[header_end + 4..].to_vec();
                while body.len() < content_length {
                    match stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => body.extend_from_slice(&chunk[..n]),
                        Err(_) => break,
                    }
                }
                let rec = RecordedRequest {
                    method,
                    target,
                    headers,
                    body: String::from_utf8_lossy(&body).to_string(),
                };
                let reply = handler(&rec);
                rec2.lock().unwrap().push(rec);
                if reply.hang {
                    thread::sleep(Duration::from_secs(3));
                    continue;
                }
                let reason = match reply.status {
                    200 => "OK",
                    404 => "Not Found",
                    500 => "Internal Server Error",
                    _ => "Status",
                };
                let mut resp = format!("HTTP/1.1 {} {}\r\n", reply.status, reason);
                for (k, v) in &reply.headers {
                    resp.push_str(&format!("{}: {}\r\n", k, v));
                }
                resp.push_str(&format!(
                    "Content-Length: {}\r\nConnection: close\r\n\r\n",
                    reply.body.len()
                ));
                resp.push_str(&reply.body);
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        });
        (format!("http://127.0.0.1:{}", port), records)
    }

    fn plain_reply(body: &str) -> MockReply {
        MockReply {
            status: 200,
            headers: vec![("Content-Type".to_string(), "text/plain".to_string())],
            body: body.to_string(),
            hang: false,
        }
    }

    /// Req 1 (§50): http.get returns res.status (number), res.headers
    /// (object), res.body (string).
    #[test]
    fn test_job016_http_get_basic_response() {
        let (base, _recs) = spawn_mock_server(|_| plain_reply("hello world"));
        let src = r#"async function main() {
    let res = await http.get("__URL__/data")
    print(res.status)
    print(res.body)
    print(type(res.status))
    print(type(res.headers))
    print(type(res.body))
    print(res.headers)
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(
            &output[..5],
            ["200", "hello world", "number", "object", "string"]
        );
        // Header key casing is preserved exactly as the server sent it.
        let h = output[5].to_lowercase();
        assert!(h.contains("content-type"), "headers: {}", output[5]);
        assert!(h.contains("text/plain"), "headers: {}", output[5]);
    }

    /// Req 2 (§50, §50.1): post/put/patch/delete carry body + headers.
    #[test]
    fn test_job016_http_methods_with_body_and_headers() {
        let (base, recs) = spawn_mock_server(|r| MockReply {
            status: 200,
            headers: vec![],
            body: format!("got {}", r.method),
            hang: false,
        });
        let src = r#"async function main() {
    let opts = {
        headers: { "Content-Type": "application/json", "X-Token": "abc" },
        body: "{\"a\":1}"
    }
    print((await http.post("__URL__/a", opts)).body)
    print((await http.put("__URL__/b", opts)).body)
    print((await http.patch("__URL__/c", opts)).body)
    print((await http.delete("__URL__/d", opts)).body)
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["got POST", "got PUT", "got PATCH", "got DELETE"]);
        let recs = recs.lock().unwrap();
        assert_eq!(recs.len(), 4);
        let methods: Vec<&str> = recs.iter().map(|r| r.method.as_str()).collect();
        assert_eq!(methods, ["POST", "PUT", "PATCH", "DELETE"]);
        let targets: Vec<&str> = recs.iter().map(|r| r.target.as_str()).collect();
        assert_eq!(targets, ["/a", "/b", "/c", "/d"]);
        for r in recs.iter() {
            assert_eq!(r.body, r#"{"a":1}"#);
            assert!(r
                .headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("content-type") && v == "application/json"));
            assert!(r
                .headers
                .iter()
                .any(|(k, v)| k.eq_ignore_ascii_case("x-token") && v == "abc"));
        }
    }

    /// Req 3 (§50, §51): res.body is always a string — a JSON payload is
    /// never auto-parsed into an object.
    #[test]
    fn test_job016_http_body_never_auto_parsed() {
        let (base, _recs) = spawn_mock_server(|_| MockReply {
            status: 200,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: r#"{"n":1}"#.to_string(),
            hang: false,
        });
        let src = r#"async function main() {
    let res = await http.get("__URL__/j")
    print(type(res.body))
    print(res.body)
    let data = json.parse(res.body)
    print(data.n)
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["string", r#"{"n":1}"#, "1"]);
    }

    /// Req 4 (§50.1): 4xx/5xx are NOT runtime errors — res.status reads
    /// normally.
    #[test]
    fn test_job016_http_4xx_5xx_are_normal_responses() {
        let (base, _recs) = spawn_mock_server(|r| MockReply {
            status: if r.target.contains("missing") { 404 } else { 500 },
            headers: vec![],
            body: "e".to_string(),
            hang: false,
        });
        let src = r#"async function main() {
    let a = await http.get("__URL__/missing")
    print(a.status)
    let b = await http.get("__URL__/boom")
    print(b.status)
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["404", "500"]);
    }

    /// Req 5 (§59): query values are URL-encoded (incl. null → empty),
    /// existing query in the URL is preserved (appended, not replaced).
    #[test]
    fn test_job016_http_query_encoding() {
        let (base, recs) = spawn_mock_server(|_| plain_reply("ok"));
        let src = r#"async function main() {
    let res = await http.get("__URL__/s?sort=name", {
        query: { page: 2, active: true, q: "a b&c", empty: null }
    })
    print(res.status)
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["200"]);
        let recs = recs.lock().unwrap();
        assert_eq!(
            recs[0].target,
            "/s?sort=name&page=2&active=true&q=a%20b%26c&empty="
        );
    }

    /// Req 6 (§59): query that is not an object → ERROR (request never
    /// leaves the process).
    #[test]
    fn test_job016_http_query_non_object_error() {
        let (base, recs) = spawn_mock_server(|_| plain_reply("ok"));
        let src = r#"async function main() {
    try {
        let r = await http.get("__URL__/q", { query: 5 })
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
        assert!(recs.lock().unwrap().is_empty());
    }

    /// Req 7 (§60): request headers are sent exactly as given.
    #[test]
    fn test_job016_http_request_headers_sent_as_given() {
        let (base, recs) = spawn_mock_server(|_| plain_reply("ok"));
        let src = r#"async function main() {
    let res = await http.get("__URL__/h", {
        headers: { "Authorization": "Bearer token", "Accept": "application/json" }
    })
    print(res.status)
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["200"]);
        let recs = recs.lock().unwrap();
        assert!(recs[0].headers.iter().any(|(k, v)| {
            k.eq_ignore_ascii_case("authorization") && v == "Bearer token"
        }));
        assert!(recs[0]
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("accept") && v == "application/json"));
    }

    /// Req 8 (§60): headers that is not an object → ERROR.
    #[test]
    fn test_job016_http_headers_non_object_error() {
        let (base, recs) = spawn_mock_server(|_| plain_reply("ok"));
        let src = r#"async function main() {
    try {
        let r = await http.get("__URL__/h", { headers: 5 })
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
        assert!(recs.lock().unwrap().is_empty());
    }

    /// Req 9 (§60): a non-string header value → ERROR.
    #[test]
    fn test_job016_http_header_value_non_string_error() {
        let (base, recs) = spawn_mock_server(|_| plain_reply("ok"));
        let src = r#"async function main() {
    try {
        let r = await http.get("__URL__/h", { headers: { "X-Bad": 5 } })
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
        assert!(recs.lock().unwrap().is_empty());
    }

    /// Req 10 (§50.1): timeout → runtime error, catchable with try/catch
    /// (server accepts but never responds).
    #[test]
    fn test_job016_http_timeout_is_catchable_error() {
        let (base, _recs) = spawn_mock_server(|_| MockReply {
            status: 200,
            headers: vec![],
            body: String::new(),
            hang: true,
        });
        let src = r#"async function main() {
    try {
        let r = await http.get("__URL__/slow", { timeout: 150 })
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#
        .replace("__URL__", &base);
        let started = std::time::Instant::now();
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
        // The failure came from the 150 ms timeout, not a hang.
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    /// Req 11: network failure (connection refused) → runtime error,
    /// catchable.
    #[test]
    fn test_job016_http_network_failure_catchable() {
        use std::net::TcpListener;
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        drop(l); // port closed → connection refused
        let src = r#"async function main() {
    try {
        let r = await http.get("http://127.0.0.1:9nope/")
        print("no error")
    } catch e {
        print("bad url caught")
    }
    try {
        let r = await http.get("http://127.0.0.1:__PORT__/")
        print("no error")
    } catch e {
        print("refused caught")
    }
}
main()"#
        .replace("__PORT__", &port.to_string());
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["bad url caught", "refused caught"]);
    }

    /// Req 12 (§50.1 example): json.stringify as request body, json.parse
    /// on the response — no automatic conversion at the HTTP layer.
    #[test]
    fn test_job016_http_json_stringify_body_roundtrip() {
        let (base, recs) = spawn_mock_server(|r| MockReply {
            status: 200,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: r.body.clone(),
            hang: false,
        });
        let src = r#"async function main() {
    let payload = json.stringify({ nama: "Kael" })
    let res = await http.post("__URL__/users", {
        headers: { "Content-Type": "application/json" },
        body: payload
    })
    if res.status == 200 {
        let data = json.parse(res.body)
        print(data.nama)
    }
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["Kael"]);
        let recs = recs.lock().unwrap();
        assert_eq!(recs[0].body, r#"{"nama":"Kael"}"#);
    }

    /// Extra: options itself must be an object (mechanical rule; §59/§60
    /// only define field-level errors).
    #[test]
    fn test_job016_http_options_non_object_error() {
        let (base, recs) = spawn_mock_server(|_| plain_reply("ok"));
        let src = r#"async function main() {
    try {
        let r = await http.get("__URL__/o", 5)
        print("no error")
    } catch e {
        print("caught")
    }
}
main()"#
        .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["caught"]);
        assert!(recs.lock().unwrap().is_empty());
    }

    /// ISSUES §65c: mutating a response's headers object is local-only —
    /// it cannot affect the response data (status/body untouched) nor leak
    /// into later responses (next request sees fresh server headers).
    #[test]
    fn test_issue65c_headers_mutation_is_local() {
        let (base, _recs) = spawn_mock_server(|_| plain_reply("hello"));
        let src = r#"async function main() {
    let res = await http.get("__URL__/data")
    let h = res.headers
    h["X-Mutated"] = "yes"
    print(h["X-Mutated"])
    print(res.status)
    print(res.body)
    let res2 = await http.get("__URL__/data")
    try {
        print(res2.headers["X-Mutated"])
        print("leaked")
    } catch e {
        print("fresh")
    }
}
main()"#
            .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output, vec!["yes", "200", "hello", "fresh"]);
    }

    /// §50 Redirect rules (ISSUES.md §65d follow-up, spec added 2026-10-02):
    /// redirects are followed automatically and capped at 10 — an endless
    /// redirect loop ends in a catchable error.
    #[test]
    fn test_job016_http_redirect_limit_is_error() {
        let (base, _recs) = spawn_mock_server(|_| MockReply {
            status: 302,
            headers: vec![("Location".to_string(), "/loop".to_string())],
            body: String::new(),
            hang: false,
        });
        let src = r#"async function main() {
    try {
        let r = await http.get("__URL__/loop")
        print("no-error")
    } catch e {
        print(e.message)
    }
}
main()"#
            .replace("__URL__", &base);
        let output = run_source(&src).unwrap();
        assert_eq!(output.len(), 1, "{:?}", output);
        assert!(output[0].contains("redirect"), "{}", output[0]);
    }

    // ---- Job 019: validation pass before execution, CALL_NATIVE, LOAD_FUNCTION ----

    fn hand_built(constants: Vec<Constant>, instructions: Vec<Opcode>) -> BytecodeProgram {
        BytecodeProgram {
            header: BytecodeHeader::current(),
            entry_point: 0,
            constants,
            instructions,
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        }
    }

    fn one_function_body(
        constants: Vec<Constant>,
        instructions: Vec<Opcode>,
        body: Vec<Opcode>,
        entry_point: usize,
    ) -> BytecodeProgram {
        BytecodeProgram {
            header: BytecodeHeader::current(),
            entry_point: 0,
            constants,
            instructions,
            functions: vec![FunctionInfo {
                name: "f".to_string(),
                params: vec![],
                param_defaults: vec![],
                entry_point,
                local_count: 0,
                captured_vars: vec![],
                is_async: false,
            }],
            function_bodies: vec![body],
            default_bodies: vec![],
        }
    }

    #[test]
    fn test_job019_header_written_by_compiler() {
        let bc = compile_source("print(\"ran\")");
        let vm = ZeroVM::new(bc);
        assert!(vm.validate().is_ok());
    }

    #[test]
    fn test_job019_unsupported_version_rejected_before_execution() {
        let mut bc = compile_source("print(\"ran\")");
        bc.header.version = 99;
        let mut vm = ZeroVM::new(bc);
        assert_eq!(
            vm.run().unwrap_err(),
            VmError::UnsupportedBytecodeVersion(99)
        );
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job019_bad_magic_rejected_before_execution() {
        let mut bc = compile_source("print(\"ran\")");
        bc.header.magic = "NOPE".to_string();
        let mut vm = ZeroVM::new(bc);
        match vm.run() {
            Err(VmError::InvalidHeader(message)) => {
                assert!(message.contains("magic"), "{}", message)
            }
            other => panic!("expected InvalidHeader, got {:?}", other),
        }
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job019_unsupported_version_constant_is_not_a_supported_version() {
        // §125: exactly one accepted format version at a time.
        assert!(crate::compiler::SUPPORTED_BYTECODE_VERSIONS.contains(&1));
        assert!(!crate::compiler::SUPPORTED_BYTECODE_VERSIONS.contains(&2));
    }

    #[test]
    fn test_job019_unknown_opcode_rejected_before_execution() {
        let bc = hand_built(vec![], vec![Opcode::Unknown(255), Opcode::Halt]);
        let mut vm = ZeroVM::new(bc);
        assert!(matches!(vm.run().unwrap_err(), VmError::InvalidOpcode(_)));
        assert!(vm.get_output().is_empty());
        assert!(vm.validate().is_err());
    }

    #[test]
    fn test_job019_invalid_jump_in_function_body_rejected_before_execution() {
        // Main stream is valid (Halt first, so execution would stop
        // immediately); only the function body is malformed.
        let bc = one_function_body(
            vec![Constant::Null],
            vec![Opcode::Halt, Opcode::LoadConst(0), Opcode::Jump(1)],
            vec![Opcode::LoadConst(0), Opcode::Jump(99)],
            1,
        );
        let mut vm = ZeroVM::new(bc);
        assert_eq!(
            vm.run().unwrap_err(),
            VmError::InvalidJumpTarget(99)
        );
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job019_invalid_constant_in_function_body_rejected_before_execution() {
        let bc = one_function_body(
            vec![Constant::Null],
            vec![Opcode::Halt, Opcode::LoadConst(0)],
            vec![Opcode::LoadConst(9)],
            1,
        );
        let mut vm = ZeroVM::new(bc);
        assert_eq!(
            vm.run().unwrap_err(),
            VmError::InvalidConstantIndex(9)
        );
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job019_entry_point_out_of_range_rejected() {
        let bc = BytecodeProgram {
            header: BytecodeHeader::current(),
            entry_point: 5,
            constants: vec![],
            instructions: vec![Opcode::Halt],
            functions: vec![],
            function_bodies: vec![],
            default_bodies: vec![],
        };
        let mut vm = ZeroVM::new(bc);
        assert!(matches!(vm.run().unwrap_err(), VmError::InvalidBytecode(_)));
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job019_function_body_count_mismatch_rejected() {
        let mut bc = hand_built(
            vec![Constant::Null],
            vec![Opcode::Halt, Opcode::LoadConst(0)],
        );
        bc.functions = vec![FunctionInfo {
            name: "f".to_string(),
            params: vec![],
            param_defaults: vec![],
            entry_point: 1,
            local_count: 0,
            captured_vars: vec![],
            is_async: false,
        }];
        let mut vm = ZeroVM::new(bc);
        assert!(matches!(vm.run().unwrap_err(), VmError::InvalidBytecode(_)));
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job019_param_default_out_of_range_rejected() {
        let mut bc = hand_built(vec![], vec![Opcode::Halt]);
        bc.functions = vec![FunctionInfo {
            name: "f".to_string(),
            params: vec!["a".to_string()],
            param_defaults: vec![Some(9)],
            entry_point: 0,
            local_count: 0,
            captured_vars: vec![],
            is_async: false,
        }];
        bc.function_bodies = vec![vec![Opcode::Halt]];
        let mut vm = ZeroVM::new(bc);
        // Job 022: param_defaults index default_bodies, not constants.
        match vm.run().unwrap_err() {
            VmError::InvalidOperand(msg) => assert!(msg.contains("default body index 9")),
            other => panic!("expected InvalidOperand, got {:?}", other),
        }
    }

    #[test]
    fn test_job019_param_default_count_mismatch_rejected() {
        let mut bc = hand_built(vec![], vec![Opcode::Halt]);
        bc.functions = vec![FunctionInfo {
            name: "f".to_string(),
            params: vec!["a".to_string(), "b".to_string()],
            param_defaults: vec![None],
            entry_point: 0,
            local_count: 0,
            captured_vars: vec![],
            is_async: false,
        }];
        bc.function_bodies = vec![vec![Opcode::Halt]];
        let mut vm = ZeroVM::new(bc);
        assert!(matches!(vm.run().unwrap_err(), VmError::InvalidOperand(_)));
    }

    #[test]
    fn test_job019_empty_variable_operand_rejected() {
        let bc = hand_built(vec![], vec![Opcode::LoadLocal("".into()), Opcode::Halt]);
        let mut vm = ZeroVM::new(bc);
        assert!(matches!(vm.run().unwrap_err(), VmError::InvalidOperand(_)));
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job019_empty_native_operation_rejected() {
        let bc = hand_built(vec![], vec![Opcode::CallNative("".into(), 0), Opcode::Halt]);
        let mut vm = ZeroVM::new(bc);
        assert!(matches!(vm.run().unwrap_err(), VmError::InvalidOperand(_)));
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job019_empty_object_key_rejected() {
        let bc = hand_built(vec![], vec![Opcode::MakeObject(vec!["".into()]), Opcode::Halt]);
        let mut vm = ZeroVM::new(bc);
        assert!(matches!(vm.run().unwrap_err(), VmError::InvalidOperand(_)));
    }

    #[test]
    fn test_job019_make_closure_target_out_of_range_rejected() {
        let bc = hand_built(
            vec![],
            vec![Opcode::MakeClosure(7, vec![]), Opcode::Halt],
        );
        let mut vm = ZeroVM::new(bc);
        assert!(matches!(vm.run().unwrap_err(), VmError::InvalidOperand(_)));
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job019_call_native_executes_builtin() {
        let bc = hand_built(
            vec![Constant::Str("hi".to_string())],
            vec![
                Opcode::LoadConst(0),
                Opcode::CallNative("print".into(), 1),
                Opcode::Halt,
            ],
        );
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_output(), &["hi"]);
    }

    #[test]
    fn test_job019_unknown_native_operation_is_runtime_error() {
        let bc = hand_built(
            vec![],
            vec![
                Opcode::CallNative("nope".into(), 0),
                Opcode::Halt,
            ],
        );
        let mut vm = ZeroVM::new(bc);
        match vm.run() {
            Err(VmError::RuntimeError(message)) => {
                assert!(message.contains("Unknown built-in 'nope'"), "{}", message)
            }
            other => panic!("expected RuntimeError, got {:?}", other),
        }
        assert!(vm.get_output().is_empty());
    }

    #[test]
    fn test_job019_native_call_arity_error_is_runtime_error() {
        let bc = hand_built(
            vec![Constant::Null],
            vec![
                Opcode::LoadConst(0),
                Opcode::LoadConst(0),
                Opcode::CallNative("type".into(), 2),
                Opcode::Halt,
            ],
        );
        let mut vm = ZeroVM::new(bc);
        assert!(matches!(vm.run().unwrap_err(), VmError::RuntimeError(_)));
    }

    #[test]
    fn test_job019_dynamic_builtin_reference_still_works() {
        let output = run_source(
            "let f = string.upper\nprint(f(\"ab\"))\nprint(type(list.length))",
        )
        .unwrap();
        assert_eq!(output, vec!["AB", "function"]);
    }

    #[test]
    fn test_job019_unknown_method_error_message_unchanged() {
        let err = run_source("print(string.bogus(\"x\"))").unwrap_err();
        assert!(
            err.to_string().contains("Property 'bogus' not found"),
            "{}",
            err
        );
    }

    #[test]
    fn test_job019_shadowed_builtin_parameter_is_used_as_local() {
        let err = run_source("function f(print) { print(7) }\nf(5)").unwrap_err();
        assert!(err.to_string().contains("Cannot call"), "{}", err);
    }

    #[test]
    fn test_job019_load_function_executes() {
        let output =
            run_source("function f() { return 42 }\nlet g = f\nprint(g())").unwrap();
        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn test_job019_load_function_recursive_still_works() {
        let output = run_source(
            "function fact(n) { if n <= 1 { return 1 } return n * fact(n - 1) }\nprint(fact(5))",
        )
        .unwrap();
        assert_eq!(output, vec!["120"]);
    }

    /// One representative call per native category after the CALL_NATIVE
    /// refactor: conversions, type, print/print.err, string, list, object,
    /// color, json, file, system, task.
    /// One representative call per native category after the CALL_NATIVE
    /// refactor: conversions, type, print, string, list, object, color,
    /// json, system, task.
    #[test]
    fn test_job019_representative_builtins_after_native_refactor() {
        let output = run_source(
            "print(type(1))\n\
             print(number(\"7\") + 1)\n\
             print(string(9))\n\
             print(boolean(true))\n\
             print(string.upper(\"ab\"))\n\
             print(string.split(\"a,b\", \",\")[1])\n\
             let A = [1]\n\
             list.add(A, 2)\n\
             print(list.length(A))\n\
             let O = { k: 1, j: 2 }\n\
             object.remove(O, \"k\")\n\
             print(O)\n\
             print(type(color.red(\"r\")))\n\
             print(json.stringify([1]))\n\
             print(type(system.os()))\n\
             async function f() { return 1 }\n\
             let t = f()\n\
             print(type(task.state(t)))\n\
             try { string.bogus(\"x\") } catch e { print(\"caught\") }",
        )
        .unwrap();
        assert_eq!(
            output,
            vec![
                "number",
                "8",
                "9",
                "true",
                "AB",
                "b",
                "2",
                "{j: 2}",
                "string",
                "[1]",
                "string",
                "string",
                "caught",
            ]
        );
    }

    #[test]
    fn test_job019_collection_instructions_execute() {
        let output = run_source(
            "let A = [1, 2]\nlist.remove(A, 0)\nprint(A[0])\n\
             let O = { k: 1, j: 2 }\nobject.remove(O, \"k\")\nprint(O)",
        )
        .unwrap();
        assert_eq!(output, vec!["2", "{j: 2}"]);
    }

    #[test]
    fn test_job019_module_programs_get_header_and_validate() {
        fn parse_program(source: &str) -> crate::ast::Program {
            let mut lexer = Lexer::new(source);
            let tokens = lexer.tokenize().unwrap();
            let mut parser = Parser::new(tokens);
            parser.parse().unwrap()
        }

        let a = crate::compiler::compile_module("./a.zs", &parse_program("print(\"a\")")).unwrap();
        let a_len = a.instructions.len();
        let b = crate::compiler::compile_module(
            "./b.zs",
            &parse_program("export function f() { return 1 }"),
        )
        .unwrap();
        let b_len = b.instructions.len();

        let merged = crate::compiler::merge_modules(&[a, b]);
        assert_eq!(merged.header.magic, crate::compiler::BYTECODE_MAGIC);
        assert_eq!(merged.entry_point, 0);
        assert_eq!(merged.functions.len(), merged.function_bodies.len());

        let modules = vec![
            crate::module::ModuleMeta {
                path: "./a.zs".to_string(),
                start: 0,
                end: a_len,
                exports: vec![],
                imports: vec![],
                re_exports: vec![],
            },
            crate::module::ModuleMeta {
                path: "./b.zs".to_string(),
                start: a_len,
                end: a_len + b_len,
                exports: vec!["f".to_string()],
                imports: vec![],
                re_exports: vec![],
            },
        ];

        let mut vm = ZeroVM::new(merged);
        vm.run_modules(&modules).unwrap();
        assert_eq!(vm.get_output(), &["a"]);
    }

    // ---- Job 023: execution limits (§150 + §133, ISSUES §71) ----

    /// Run `source` under the given limits. Returns the output captured up
    /// to termination plus the run result — output matters for the §153
    /// "no instruction after termination" assertions.
    fn run_source_with_limits(
        source: &str,
        limits: Limits,
    ) -> (Vec<String>, Result<(), VmError>) {
        let bc = compile_source(source);
        let mut vm = ZeroVM::new(bc);
        vm.set_limits(limits).unwrap();
        let result = vm.run();
        (vm.get_output().to_vec(), result)
    }

    #[test]
    fn test_job023_call_depth_limit_is_catchable() {
        let src = "function recur() {\n    return recur()\n}\ntry {\n    recur()\n    print(\"no-error\")\n} catch e {\n    print(e.type)\n    print(string(e))\n}";
        let (output, result) = run_source_with_limits(
            src,
            Limits {
                call_stack_depth: Some(40),
                ..Default::default()
            },
        );
        result.unwrap();
        assert_eq!(output[0], "LimitError");
        assert!(
            output[1].contains("call stack depth"),
            "{:?}",
            output[1]
        );
    }

    #[test]
    fn test_job023_call_depth_limit_uncaught_is_error() {
        let src = "function recur() {\n    return recur()\n}\nrecur()";
        let (output, result) = run_source_with_limits(
            src,
            Limits {
                call_stack_depth: Some(30),
                ..Default::default()
            },
        );
        match result {
            Err(VmError::RuntimeError(msg)) => {
                assert!(msg.contains("Execution limit exceeded"), "{}", msg);
                assert!(msg.contains("call stack depth"), "{}", msg);
            }
            other => panic!("expected limit runtime error, got {:?}", other),
        }
        assert!(output.is_empty());
    }

    #[test]
    fn test_job023_instruction_limit_stops_infinite_loop() {
        let src = "let i = 0\nwhile true { i = i + 1 }";
        let (_output, result) = run_source_with_limits(
            src,
            Limits {
                instruction_count: Some(5000),
                ..Default::default()
            },
        );
        match result {
            Err(VmError::RuntimeError(msg)) => {
                assert!(msg.contains("instruction count"), "{}", msg)
            }
            other => panic!("expected instruction limit error, got {:?}", other),
        }
    }

    #[test]
    fn test_job023_instruction_limit_under_budget_completes() {
        let (output, result) = run_source_with_limits(
            "print(\"ok\")",
            Limits {
                instruction_count: Some(1_000_000),
                ..Default::default()
            },
        );
        result.unwrap();
        assert_eq!(output, vec!["ok"]);
    }

    #[test]
    fn test_job023_execution_time_limit_stops_busy_loop() {
        let src = "while true { let x = 1 }";
        let (_output, result) = run_source_with_limits(
            src,
            Limits {
                execution_time_ms: Some(30),
                ..Default::default()
            },
        );
        match result {
            Err(VmError::RuntimeError(msg)) => {
                assert!(msg.contains("wall clock time"), "{}", msg)
            }
            other => panic!("expected wall-clock limit error, got {:?}", other),
        }
    }

    #[test]
    fn test_job023_execution_time_limit_is_catchable() {
        let src = "try {\n    while true {\n        let x = 1\n    }\n} catch e {\n    print(e.type)\n}";
        let (output, result) = run_source_with_limits(
            src,
            Limits {
                execution_time_ms: Some(30),
                ..Default::default()
            },
        );
        result.unwrap();
        assert_eq!(output, vec!["LimitError"]);
    }

    #[test]
    fn test_job023_execution_time_under_budget_completes() {
        let (output, result) = run_source_with_limits(
            "print(\"ok\")",
            Limits {
                execution_time_ms: Some(60_000),
                ..Default::default()
            },
        );
        result.unwrap();
        assert_eq!(output, vec!["ok"]);
    }

    #[test]
    fn test_job023_task_count_limit_is_error() {
        let src = "async function f() {\n    return 1\n}\nlet a = f()\nlet b = f()";
        let (output, result) = run_source_with_limits(
            src,
            Limits {
                task_count: Some(1),
                ..Default::default()
            },
        );
        match result {
            Err(VmError::RuntimeError(msg)) => {
                assert!(msg.contains("task count"), "{}", msg)
            }
            other => panic!("expected task count limit error, got {:?}", other),
        }
        assert!(output.is_empty());
    }

    /// ISSUES §71e: the limit counts non-terminal tasks, so a program that
    /// awaits tasks to completion may create more tasks than the limit over
    /// the lifetime of the run (terminal tasks never release table slots).
    #[test]
    fn test_job023_task_count_counts_only_active_tasks() {
        let src = "async function f() {\n    return 1\n}\nasync function main() {\n    let a = f()\n    await a\n    let b = f()\n    await b\n    let c = f()\n    print(\"ok\")\n}\nmain()";
        let (output, result) = run_source_with_limits(
            src,
            Limits {
                task_count: Some(2),
                ..Default::default()
            },
        );
        result.unwrap();
        assert_eq!(output, vec!["ok"]);
    }

    /// §133: exceeding the heap cap is a VM Error — try/catch must not catch
    /// it (ISSUES §71a), and no instruction runs after the termination (§153).
    #[test]
    fn test_job023_memory_limit_exceeded_is_vm_error_not_catchable() {
        let src = "let k = [1]\ntry {\n    while true {\n        list.add(k, [1])\n    }\n} catch e {\n    print(\"caught\")\n}\nprint(\"after\")";
        let (output, result) = run_source_with_limits(
            src,
            Limits {
                heap_slots: Some(40),
                ..Default::default()
            },
        );
        match result {
            Err(VmError::MemoryLimit(msg)) => {
                assert!(msg.contains("heap slots"), "{}", msg)
            }
            other => panic!("expected memory limit VM error, got {:?}", other),
        }
        assert!(output.is_empty(), "{:?}", output);
    }

    /// §133/§71d: GC runs before the allocation is declared failed, so
    /// reclaimable garbage does not count — the same cap that a live-growth
    /// program dies on lets 400 garbage iterations through.
    #[test]
    fn test_job023_memory_limit_gc_frees_garbage_before_failing() {
        let src = "let i = 0\nwhile i < 400 {\n    let g = [i]\n    i = i + 1\n}\nprint(\"done\")";
        let (output, result) = run_source_with_limits(
            src,
            Limits {
                heap_slots: Some(40),
                ..Default::default()
            },
        );
        result.unwrap();
        assert_eq!(output, vec!["done"]);
    }

    /// §149: a zero budget is invalid configuration — rejected by the setter
    /// and again at run entry before the first instruction (ISSUES §71f).
    #[test]
    fn test_job023_invalid_limit_is_startup_error() {
        let bc = compile_source("print(1)");
        let mut vm = ZeroVM::new(bc);
        for bad in [
            Limits {
                execution_time_ms: Some(0),
                ..Default::default()
            },
            Limits {
                heap_slots: Some(0),
                ..Default::default()
            },
            Limits {
                call_stack_depth: Some(0),
                ..Default::default()
            },
            Limits {
                task_count: Some(0),
                ..Default::default()
            },
            Limits {
                instruction_count: Some(0),
                ..Default::default()
            },
        ] {
            assert!(matches!(
                vm.set_limits(bad),
                Err(VmError::InvalidRuntimeConfiguration(_))
            ));
        }
        // Defense in depth: a value that bypasses the setter is caught at
        // run entry, before any instruction executes.
        vm.limits.heap_slots = Some(0);
        let result = vm.run();
        assert!(matches!(
            result,
            Err(VmError::InvalidRuntimeConfiguration(_))
        ));
        assert!(vm.get_output().is_empty());
    }

    /// §153: once the limit terminates the program, no further instruction
    /// executes — the trailing print never runs.
    #[test]
    fn test_job023_no_instruction_runs_after_limit_termination() {
        let src = "let i = 0\nwhile true { i = i + 1 }\nprint(\"after\")";
        let (output, result) = run_source_with_limits(
            src,
            Limits {
                instruction_count: Some(3000),
                ..Default::default()
            },
        );
        assert!(matches!(result, Err(VmError::RuntimeError(_))));
        assert!(output.is_empty(), "{:?}", output);
    }

    #[test]
    fn test_job023_unlimited_by_default_completes() {
        let (output, result) = run_source_with_limits("print(\"ok\")", Limits::default());
        result.unwrap();
        assert_eq!(output, vec!["ok"]);
    }

    /// ISSUES §71b: while HTTP is on the wire the scheduler parks in real
    /// time — the wall-clock limit still fires there (no instruction
    /// boundary runs), and with no context loaded it surfaces uncaught
    /// (ISSUES §71a).
    #[test]
    fn test_job023_time_limit_trips_while_waiting_for_http() {
        let (base, _recs) = spawn_mock_server(|_| {
            thread::sleep(Duration::from_millis(300));
            plain_reply("late")
        });
        let src = r#"async function main() {
    let res = await http.get("__URL__/slow")
    print(res.status)
}
main()"#
            .replace("__URL__", &base);
        let (output, result) = run_source_with_limits(
            &src,
            Limits {
                execution_time_ms: Some(50),
                ..Default::default()
            },
        );
        match result {
            Err(VmError::RuntimeError(msg)) => {
                assert!(msg.contains("wall clock time"), "{}", msg)
            }
            other => panic!("expected wall-clock limit error, got {:?}", other),
        }
        assert!(output.is_empty(), "{:?}", output);
    }

    // ===== Job 027: value stack local per execution frame (ZeroVM.md §116) =====

    /// §116 "Value Stack bersifat lokal terhadap execution frame": after a
    /// program whose values go through function calls, the root frame's
    /// stack must equal the stack of the same computation without calls —
    /// no callee temporary may remain visible anywhere.
    #[test]
    fn test_job027_value_stack_is_frame_local() {
        let bc_calls = compile_source(
            "function junk() {\n    let t = 1 + 2\n    let u = t * 3\n    return u\n}\nlet a = junk()\nprint(a)",
        );
        let mut vm_calls = ZeroVM::new(bc_calls);
        vm_calls.run().unwrap();
        assert_eq!(vm_calls.get_output(), ["9"]);
        assert_eq!(*vm_calls.get_state(), ExecutionState::Completed);

        let bc_direct = compile_source("let a = 9\nprint(a)");
        let mut vm_direct = ZeroVM::new(bc_direct);
        vm_direct.run().unwrap();

        assert_eq!(
            vm_calls.get_stack(),
            vm_direct.get_stack(),
            "callee temporaries leaked into the visible stack: {:?}",
            vm_calls.get_stack()
        );
    }

    /// §116/§117: deep recursion creates many frames, each carrying its own
    /// value stack; the root frame still ends clean and the result correct.
    #[test]
    fn test_job027_recursive_frames_each_own_stack() {
        let bc = compile_source(
            "function sum(n) {\n    if n <= 0 {\n        return 0\n    }\n    return n + sum(n - 1)\n}\nprint(sum(50))",
        );
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_output(), ["1275"]);
        assert_eq!(*vm.get_state(), ExecutionState::Completed);

        let bc_direct = compile_source("print(1275)");
        let mut vm_direct = ZeroVM::new(bc_direct);
        vm_direct.run().unwrap();
        assert_eq!(vm.get_stack(), vm_direct.get_stack());
    }

    /// §116 + §115 unwinding: list elements pushed before a throw are
    /// residue inside the handler's own frame; catch must truncate that
    /// frame's stack back to the try-entry depth (cross-frame throw pops
    /// the callee first), leaving no residue on return.
    #[test]
    fn test_job027_unwind_truncates_handler_frame_stack() {
        let bc = compile_source(
            "function boom() {\n    throw \"bad\"\n}\nfunction f() {\n    try {\n        let l = [1, 2, boom(), 4]\n        return 1\n    } catch e {\n        return 2\n    }\n}\nprint(f())",
        );
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_output(), ["2"]);
        assert_eq!(*vm.get_state(), ExecutionState::Completed);

        let bc_direct = compile_source("print(2)");
        let mut vm_direct = ZeroVM::new(bc_direct);
        vm_direct.run().unwrap();
        assert_eq!(
            vm.get_stack(),
            vm_direct.get_stack(),
            "unwind left residue: {:?}",
            vm.get_stack()
        );
    }

    /// §121 "nilai pada Value Stack" under §116 locality: between building
    /// the list and returning it, the value exists only on the callee
    /// frame's own stack — gc_roots must find it there (threshold 0 = GC
    /// at every instruction boundary).
    #[test]
    fn test_job027_callee_frame_stack_roots_survive_gc() {
        let (out, collections, _) = run_source_with_gc_pressure(
            "function make() {\n    return [7, 8, 9]\n}\nfunction first(l) {\n    return l[0]\n}\nprint(first(make()))",
            0,
        )
        .unwrap();
        assert_eq!(out, vec!["7"]);
        assert!(collections > 0, "GC must actually run during execution");
    }

    // ===== Switch runtime behavior (bug found while building examples/demo.zs) =====

    /// Multi-case switch: the value must reach the matching case, not the
    /// default — JumpIfFalse peeks, so a missed case previously left a
    /// stale `false` that the next case duplicated and compared instead.
    #[test]
    fn test_switch_matches_second_case() {
        let output = run_source(
            "let hari = 2\nswitch hari {\n    case 1 {\n        print(\"c1\")\n    }\n    case 2 {\n        print(\"c2\")\n    }\n    default {\n        print(\"def\")\n    }\n}",
        )
        .unwrap();
        assert_eq!(output, vec!["c2"]);
    }

    #[test]
    fn test_switch_default_when_no_case_matches() {
        let output = run_source(
            "let x = 9\nswitch x {\n    case 1 {\n        print(\"c1\")\n    }\n    case 2 {\n        print(\"c2\")\n    }\n    default {\n        print(\"def\")\n    }\n}",
        )
        .unwrap();
        assert_eq!(output, vec!["def"]);
    }

    /// A matched switch leaves nothing on the caller's value stack — any
    /// residue breaks the balance of the statements that follow.
    #[test]
    fn test_switch_stack_balanced_after_match() {
        let bc = compile_source(
            "let x = 2\nswitch x {\n    case 1 {\n        print(\"c1\")\n    }\n    case 2 {\n        print(\"c2\")\n    }\n    default {\n        print(\"def\")\n    }\n}\nprint(x + 1)",
        );
        let mut vm = ZeroVM::new(bc);
        vm.run().unwrap();
        assert_eq!(vm.get_output(), ["c2", "3"]);

        let bc_direct = compile_source("let x = 2\nprint(x + 1)");
        let mut vm_direct = ZeroVM::new(bc_direct);
        vm_direct.run().unwrap();
        assert_eq!(
            vm.get_stack(),
            vm_direct.get_stack(),
            "switch leaked stack residue: {:?}",
            vm.get_stack()
        );
    }

    // ===== Job 029: if/while dalam function tidak boleh mematikan range modul =====
    // Jalur module (compile_module + run_modules) = jalur `zsc file.zs` yang
    // terbukti rusak: run_module_range memakai `end` top-level walau pc sudah
    // berada di ruang-alamat body function (instructions di-swap saat Call).

    fn run_as_module(source: &str) -> Vec<String> {
        fn parse_program(source: &str) -> crate::ast::Program {
            let mut lexer = Lexer::new(source);
            let tokens = lexer.tokenize().unwrap();
            let mut parser = Parser::new(tokens);
            parser.parse().unwrap()
        }
        let unit =
            crate::compiler::compile_module("./main.zs", &parse_program(source)).unwrap();
        let end = unit.instructions.len();
        let merged = crate::compiler::merge_modules(&[unit]);
        let modules = vec![crate::module::ModuleMeta {
            path: "./main.zs".to_string(),
            start: 0,
            end,
            exports: vec![],
            imports: vec![],
            re_exports: vec![],
        }];
        let mut vm = ZeroVM::new(merged);
        vm.run_modules(&modules).unwrap();
        vm.get_output().to_vec()
    }

    #[test]
    fn test_job029_if_in_function_returns_to_caller() {
        let out = run_as_module(
            "function f() {\n    if true {\n        print(\"in\")\n    }\n    return 2\n}\nprint(f())\nprint(\"top\")",
        );
        assert_eq!(out, vec!["in", "2", "top"]);
    }

    #[test]
    fn test_job029_while_false_in_function_continues_top_level() {
        let out = run_as_module(
            "function f() {\n    while false { }\n    print(\"after\")\n    return 1\n}\nf()\nprint(\"top\")",
        );
        assert_eq!(out, vec!["after", "top"]);
    }

    #[test]
    fn test_job029_while_iterations_return_sum() {
        let out = run_as_module(
            "function sum5() {\n    let i = 0\n    let t = 0\n    while i < 5 {\n        t = t + i\n        i = i + 1\n    }\n    return t\n}\nprint(sum5())\nprint(\"end\")",
        );
        assert_eq!(out, vec!["10", "end"]);
    }

    #[test]
    fn test_job029_while_break_without_try_returns() {
        let out = run_as_module(
            "function f() {\n    while true {\n        print(\"b\")\n        break\n    }\n    return 7\n}\nprint(f())\nprint(\"top\")",
        );
        assert_eq!(out, vec!["b", "7", "top"]);
    }

    #[test]
    fn test_job029_if_while_guard_on_run_path() {
        // Guard jalur non-module (vm.run) tetap menghasilkan output lengkap.
        let out = run_source(
            "function f() {\n    if true {\n        print(\"in\")\n    }\n    return 2\n}\nprint(f())\nprint(\"top\")",
        );
        assert_eq!(out.unwrap(), vec!["in", "2", "top"]);
    }

    #[test]
    fn test_job030_local_store_create_then_cached_hits() {
        // Bare assignment creates the local on first store; later iterations
        // must hit the per-pc cache and still see the same binding.
        let out = run_as_module(
            "function g() {\n    let i = 0\n    x = 1\n    while i < 3 {\n        i = i + 1\n        x = x + 1\n    }\n    return x\n}\nprint(g())",
        );
        assert_eq!(out, vec!["4"]);
    }

    #[test]
    fn test_job030_global_store_cached_in_loop() {
        // Jalur run(): top-level let = DeclareGlobal/StoreGlobal; StoreGlobal
        // di dalam loop resolve sekalu lalu hit per pc; LoadGlobal print juga.
        let out = run_source(
            "let g = 0\nlet i = 0\nwhile i < 3 {\n    i = i + 1\n    g = i\n}\nprint(g)",
        );
        assert_eq!(out.unwrap(), vec!["3"]);
    }

    #[test]
    fn test_job030_module_let_store_local_cached_in_loop() {
        // Jalur module: top-level let memakai StoreLocal; binding yang sama
        // diakses dari loop harus ter-cache dengan distance benar.
        let out = run_as_module(
            "let i = 0\nwhile i < 3 {\n    i = i + 1\n}\nprint(i)",
        );
        assert_eq!(out, vec!["3"]);
    }

    #[test]
    fn test_job030_cached_local_shadowing_dist() {
        // Dua pc dengan distance berbeda pada nama yang sama (shadowing):
        // cache tidak boleh menyilangkan hasil resolve.
        let out = run_source(
            "function h() {\n    let v = 100\n    let r = 0\n    if true {\n        let v = 1\n        r = r + v\n    }\n    r = r + v\n    return r\n}\nprint(h())",
        );
        assert_eq!(out.unwrap(), vec!["101"]);
    }

    #[test]
    fn test_job030_cache_correct_across_call_swap() {
        // Tiap Call/Return mengganti instruction stream (cache_gen bump);
        // resolusi ulang setelah swap harus identik dengan resolve awal.
        let out = run_source(
            "let a = 5\nfunction f(n) {\n    let r = n + a\n    return r\n}\nprint(f(1))\nprint(f(10))\nprint(a)",
        );
        assert_eq!(out.unwrap(), vec!["6", "15", "5"]);
    }

    #[test]
    fn test_job032_top_level_return_module_path() {
        // ISSUES §75e / opsi B: top-level return ends the program; output
        // before it is kept, code after it never runs, no "No root frame".
        let out = run_as_module("print(\"a\")\nreturn 42\nprint(\"b\")");
        assert_eq!(out, vec!["a"]);
    }

    #[test]
    fn test_job032_top_level_return_run_path() {
        let out = run_source("print(\"a\")\nreturn 42\nprint(\"b\")");
        assert_eq!(out.unwrap(), vec!["a"]);
    }

    #[test]
    fn test_job032_top_level_bare_return() {
        // Bare return di top level (sebelum EOF) mengakhiri program.
        // Catatan: `return\n<stmt>` memakan statement berikutnya sbg expression
        // (parser quirk pre-existing — dicatat sbg kandidat gap CHECK, terpisah
        // dari job ini).
        let out = run_source("print(\"a\")\nreturn");
        assert_eq!(out.unwrap(), vec!["a"]);
    }

    #[test]
    fn test_job032_function_return_unchanged() {
        // Aturan fungsi tidak berubah: nilai tetap dikembalikan ke pemanggil.
        let out = run_source("function f() {\n    return 7\n}\nprint(f())\nprint(\"after\")");
        assert_eq!(out.unwrap(), vec!["7", "after"]);
    }

    #[test]
    fn test_job032_top_level_return_runs_finally() {
        // Jalur complete_pending_return: finally yang terlewati tetap jalan.
        let out = run_source("try {\n    return 1\n} finally {\n    print(\"f\")\n}\nprint(\"never\")");
        assert_eq!(out.unwrap(), vec!["f"]);
    }
