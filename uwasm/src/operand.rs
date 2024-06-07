use crate::interpreter::{InterpreterError, Serializer, VmStack};
use crate::parser::TypeKind;

pub trait Operand: Copy {
    const TYPE: TypeKind;

    fn pop(stack: &mut VmStack) -> Result<Self, InterpreterError>;

    fn push(stack: &mut VmStack, value: Self);

    fn write_to(&self, serializer: &mut Serializer);
}

impl Operand for i32 {
    const TYPE: TypeKind = TypeKind::I32;

    fn pop(stack: &mut VmStack) -> Result<Self, InterpreterError> {
        stack.pop_i32()
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i32(value)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&self.to_ne_bytes());
    }
}

impl Operand for u32 {
    const TYPE: TypeKind = TypeKind::I32;

    fn pop(stack: &mut VmStack) -> Result<Self, InterpreterError> {
        stack.pop_i32().map(|s| s as u32)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i32(value as i32)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&self.to_ne_bytes());
    }
}

impl Operand for i64 {
    const TYPE: TypeKind = TypeKind::I64;

    fn pop(stack: &mut VmStack) -> Result<Self, InterpreterError> {
        stack.pop_i64()
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i64(value)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&self.to_ne_bytes());
    }
}

impl Operand for u64 {
    const TYPE: TypeKind = TypeKind::I64;

    fn pop(stack: &mut VmStack) -> Result<Self, InterpreterError> {
        stack.pop_i64().map(|s| s as u64)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i64(value as i64)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&self.to_ne_bytes());
    }
}

impl Operand for f32 {
    const TYPE: TypeKind = TypeKind::F32;

    fn pop(stack: &mut VmStack) -> Result<Self, InterpreterError> {
        stack.pop_f32()
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_f32(value)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&self.to_ne_bytes());
    }
}

impl Operand for f64 {
    const TYPE: TypeKind = TypeKind::F64;
    fn pop(stack: &mut VmStack) -> Result<Self, InterpreterError> {
        stack.pop_f64()
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_f64(value)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&self.to_ne_bytes());
    }
}

impl Operand for bool {
    const TYPE: TypeKind = TypeKind::Void;

    fn pop(stack: &mut VmStack) -> Result<Self, InterpreterError> {
        stack.pop_i32().map(|s| s != 0)
    }

    fn push(stack: &mut VmStack, value: Self) {
        stack.push_i32(value as i32)
    }

    fn write_to(&self, serializer: &mut Serializer) {
        serializer.write_bytes(&[*self as u8]);
    }
}